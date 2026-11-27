//! `actserve edge-deploy` through the real binary: ssh, scp and actesskey are scripts.
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
        let dir = std::env::temp_dir().join(format!("actserve-edge-{}-{name}", std::process::id()));
        std::fs::create_dir_all(dir.join("edge")).unwrap();
        std::fs::write(dir.join("servers.txt"), "acteble|api.acteble|host-1\n").unwrap();
        std::fs::write(dir.join("edge").join("Caddyfile"), "x").unwrap();
        std::fs::write(dir.join("edge").join("Caddyfile.socket"), "x").unwrap();
        std::fs::create_dir_all(dir.join("edge/sites")).unwrap();
        std::fs::create_dir_all(dir.join("edge/quadlet")).unwrap();
        script(&dir.join("ssh"), SSH);
        script(
            &dir.join("scp"),
            "echo \"scp $*\" >> \"$LOG\"; exit ${SCP_RC:-0}",
        );
        script(&dir.join("actesskey"), r#"printf 'value-of-%s\n' "$2""#);
        Self { dir }
    }
    fn run(&self, args: &[&str], extra: &[(&str, &str)]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_actserve"));
        c.args(args)
            .env("ACTCONF_STATE_ROOT", &self.dir)
            .env("ACTSERVE_SSH", self.dir.join("ssh"))
            .env("ACTSERVE_SCP", self.dir.join("scp"))
            .env("ACTSERVE_ACTESSKEY", self.dir.join("actesskey"))
            .env("ACTEBLE_EDGE_DIR", self.dir.join("edge"))
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
cat >> "$LOG"
case "$*" in *"./edge up"*) exit ${START_RC:-0};; esac
exit 0"#;

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

#[test]
fn a_deploy_runs_the_five_steps_and_prints_them() {
    let e = Env::new("ok");
    let out = e.run(&["edge-deploy", "acteble"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        text(&out.stdout),
        "edge-deploy prepare   applied\nedge-deploy copy      applied\nedge-deploy units     applied\nedge-deploy binary    applied\nedge-deploy start     applied\n"
    );
    let l = e.log();
    assert!(l.contains("api.acteble:/opt/acteble/edge/"), "{l}");
    assert!(l.contains("value-of-acteble-edge-prod") && l.contains("value-of-ghcr-read-packages"));
    assert!(
        !l.lines()
            .filter(|x| x.starts_with("ssh"))
            .any(|x| x.contains("value-of"))
    );
}

#[test]
fn a_dry_run_runs_nothing_and_exits_0() {
    let e = Env::new("dry");
    let out = e.run(&["edge-deploy", "--dry-run", "acteble"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(text(&out.stdout).contains("edge-deploy start     would apply"));
    assert_eq!(e.log(), "");
}

#[test]
fn a_missing_edge_file_exits_1_naming_the_variable_and_runs_nothing() {
    let e = Env::new("nofile");
    std::fs::remove_dir_all(e.dir.join("edge/sites")).unwrap();
    let out = e.run(&["edge-deploy", "acteble"], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("ACTEBLE_EDGE_DIR"), "{out:?}");
    assert_eq!(e.log(), "");
}

#[test]
fn a_failed_copy_exits_1_before_any_secret_reaches_the_server() {
    let e = Env::new("copyfail");
    let out = e.run(&["edge-deploy", "acteble"], &[("SCP_RC", "1")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stdout).contains("edge-deploy copy      fail ("));
    assert!(!e.log().contains("value-of"));
}

#[test]
fn a_failed_start_step_exits_1_and_the_env_file_is_still_removed() {
    let e = Env::new("startfail");
    let out = e.run(&["edge-deploy", "acteble"], &[("START_RC", "9")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stdout).contains("edge-deploy start     fail (exit 9"),
        "{out:?}"
    );
    assert!(
        e.log()
            .trim_end()
            .ends_with("rm -f '/dev/shm/acteble-edge.env'")
    );
}

#[test]
fn another_host_exits_3_and_usage_errors_exit_2() {
    let e = Env::new("wrong");
    script(&e.dir.join("ssh"), "echo other-host");
    assert_eq!(
        e.run(&["edge-deploy", "acteble"], &[]).status.code(),
        Some(3)
    );
    let e = Env::new("usage");
    assert_eq!(e.run(&["edge-deploy", "nope"], &[]).status.code(), Some(2));
    assert_eq!(e.run(&["edge-deploy"], &[]).status.code(), Some(2));
    assert_eq!(
        e.run(&["edge-deploy", "--x", "acteble"], &[]).status.code(),
        Some(2)
    );
    assert_eq!(
        e.run(&["edge-deploy", "a", "b"], &[]).status.code(),
        Some(2)
    );
}
