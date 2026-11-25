//! `actserve cdn-build-geo` through the real binary: ssh and actesskey are scripts.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

struct Env {
    dir: PathBuf,
}
impl Env {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("actserve-geo-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("servers.txt"), "acteble|api.acteble|host-1\n").unwrap();
        script(&dir.join("ssh"), SSH);
        script(
            &dir.join("actesskey"),
            r#"if [ "$2" = acteble-cdn-prod ] && [ -z "$NOKEY" ]; then printf 'CDN_MANIFEST_SIGNING_KEY=key-1\n'; else printf 'value-of-%s\n' "$2"; fi"#,
        );
        Self { dir }
    }
    fn run(&self, args: &[&str], extra: &[(&str, &str)]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_actserve"));
        c.args(args)
            .env("ACTCONF_STATE_ROOT", &self.dir)
            .env("DOTFILES_SSH", self.dir.join("ssh"))
            .env("ACTSERVE_ACTESSKEY", self.dir.join("actesskey"))
            .env("LOG", self.dir.join("log"));
        for (k, v) in extra {
            c.env(k, v);
        }
        c.output().unwrap()
    }
    fn log(&self) -> String {
        std::fs::read_to_string(self.dir.join("log")).unwrap_or_default()
    }
}

/// Answers the hostname probe; logs every other call (arguments, then stdin).
const SSH: &str = r#"case "$*" in *" hostname") echo host-1; exit 0;; esac
echo "ssh $*" >> "$LOG"
cat >> "$LOG" < /dev/null
case "$*" in *"podman logs -f"*) echo "build-line"; exit ${BUILD_RC:-0};; esac
case "$*" in *"--stage"*) exit ${START_RC:-0};; esac
exit 0"#;

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

#[test]
fn a_build_starts_and_follows_then_prints_both_steps() {
    let e = Env::new("ok");
    let out = e.run(&["cdn-build-geo", "acteble"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let o = text(&out.stdout);
    assert!(o.contains("start") && o.contains("follow"), "{o}");
    assert!(e.log().contains("--stage all"), "default stage is all");
}

#[test]
fn the_stage_flag_reaches_the_server() {
    let e = Env::new("stage");
    let out = e.run(&["cdn-build-geo", "acteble", "--stage", "tree"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(e.log().contains("--stage tree"), "{}", e.log());
}

#[test]
fn a_bad_stage_exits_1_before_any_call() {
    let e = Env::new("bad");
    let out = e.run(&["cdn-build-geo", "acteble", "--stage", "nope"], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("catalog|tree|publish|all"));
    assert_eq!(e.log(), "");
}

#[test]
fn a_dry_run_runs_nothing_and_exits_0() {
    let e = Env::new("dry");
    let out = e.run(&["cdn-build-geo", "acteble", "--dry-run"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(text(&out.stdout).contains("start"));
    assert_eq!(e.log(), "");
}

#[test]
fn a_missing_signing_key_exits_1_and_touches_nothing() {
    let e = Env::new("nokey");
    let out = e.run(&["cdn-build-geo", "acteble"], &[("NOKEY", "1")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stdout).contains("CDN_MANIFEST_SIGNING_KEY"));
    assert_eq!(e.log(), "");
}

#[test]
fn a_failed_start_exits_1_and_does_not_follow() {
    let e = Env::new("start");
    let out = e.run(&["cdn-build-geo", "acteble"], &[("START_RC", "1")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(!e.log().contains("podman logs -f"));
}

#[test]
fn a_failed_build_exits_1() {
    let e = Env::new("build");
    let out = e.run(&["cdn-build-geo", "acteble"], &[("BUILD_RC", "4")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stdout).contains("exit 4"));
}

#[test]
fn another_host_exits_3_and_runs_nothing() {
    let e = Env::new("wrong");
    script(&e.dir.join("ssh"), "echo other-box");
    let out = e.run(&["cdn-build-geo", "acteble"], &[]);
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    assert_eq!(e.log(), "");
}

#[test]
fn an_unknown_name_exits_2() {
    let e = Env::new("unk");
    let out = e.run(&["cdn-build-geo", "nope"], &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("nope"));
}

#[test]
fn usage_errors_exit_2_and_never_start_the_server() {
    let e = Env::new("usage");
    for args in [
        vec!["cdn-build-geo"],
        vec!["cdn-build-geo", "a", "b"],
        vec!["cdn-build-geo", "acteble", "--stage"],
        vec!["cdn-build-geo", "acteble", "--wat"],
        vec!["cdn-build-geo", "--stage", "all"],
        vec!["cdn-build-geo", "--wat"],
        vec!["cdn-build-geo", "-n"],
    ] {
        let out = e.run(&args, &[]);
        assert_eq!(out.status.code(), Some(2), "{args:?} {out:?}");
        assert!(
            text(&out.stderr).contains("usage: actserve cdn-build-geo NAME"),
            "{args:?}"
        );
    }
}
