//! `actserve deploy` and `actserve rollback` through the real binary: ssh, scp and actesskey are scripts.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod common;

use actserve::domain::api_podman::API_UNIT_FILES;

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

struct Env {
    dir: PathBuf,
}
impl Env {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("actserve-dep-{}-{name}", std::process::id()));
        std::fs::create_dir_all(dir.join("api")).unwrap();
        std::fs::write(dir.join("servers.txt"), "acteble|api.acteble|host-1\n").unwrap();
        for f in API_UNIT_FILES {
            let path = dir.join("api").join(f);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        script(&dir.join("ssh"), SSH);
        script(
            &dir.join("scp"),
            "echo \"scp $*\" >> \"$LOG\"; exit ${SCP_RC:-0}",
        );
        // The prod env is a full env file; any other secret is `value-of-<name>`.
        std::fs::write(dir.join("prod.env"), common::full_api_env()).unwrap();
        script(
            &dir.join("actesskey"),
            &format!(
                "case \"$2\" in acteble-api-prod) cat '{}';; *) printf 'value-of-%s\\n' \"$2\";; esac",
                dir.join("prod.env").display()
            ),
        );
        Self { dir }
    }
    fn run(&self, args: &[&str], extra: &[(&str, &str)]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_actserve"));
        c.args(args)
            .env("ACTCONF_STATE_ROOT", &self.dir)
            .env("ACTSERVE_SSH", self.dir.join("ssh"))
            .env("ACTSERVE_SCP", self.dir.join("scp"))
            .env("ACTSERVE_ACTESSKEY", self.dir.join("actesskey"))
            .env("ACTEBLE_API_DIR", self.dir.join("api"))
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
case "$*" in *"podman login"*) exit ${RESTART_RC:-0};; esac
exit 0"#;

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

#[test]
fn a_deploy_prepares_copies_then_restarts_and_prints_the_steps() {
    let e = Env::new("ok");
    let out = e.run(&["deploy", "acteble"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        text(&out.stdout),
        "deploy prepare   applied\ndeploy copy      applied\ndeploy restart   applied\n"
    );
    let l = e.log();
    let (m, c, w, r, d) = (
        l.find("mkdir -p '/opt/acteble/api'").unwrap(),
        l.find("scp -o BatchMode=yes").unwrap(),
        l.find("cat > '/dev/shm/acteble-api.env'").unwrap(),
        l.find("podman login").unwrap(),
        l.find("rm -f '/dev/shm/acteble-api.env'").unwrap(),
    );
    assert!(m < c && c < w && w < r && r < d, "{l}");
    assert!(l.contains("api.acteble:.config/containers/systemd/"), "{l}");
    assert!(l.contains("POSTGRES_PASSWORD=v") && l.contains("value-of-ghcr-read-packages"));
    assert!(
        !l.lines()
            .filter(|x| x.starts_with("ssh"))
            .any(|x| x.contains("value-of") || x.contains("POSTGRES_PASSWORD=v"))
    );
}

#[test]
fn an_env_missing_a_required_key_exits_1_naming_it_and_the_server_is_untouched() {
    let e = Env::new("badenv");
    std::fs::write(
        e.dir.join("prod.env"),
        common::full_api_env().replace("TURNSTILE_SECRET=v\n", ""),
    )
    .unwrap();
    let out = e.run(&["deploy", "acteble"], &[]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        text(&out.stdout).contains("deploy secrets")
            && text(&out.stdout).contains("TURNSTILE_SECRET")
    );
    assert_eq!(e.log(), "");
}

#[test]
fn a_deploy_dry_run_runs_nothing_and_exits_0() {
    let e = Env::new("dry");
    let out = e.run(&["deploy", "acteble", "--dry-run"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        text(&out.stdout),
        "deploy prepare   would apply\ndeploy copy      would apply\ndeploy restart   would apply\n"
    );
    assert_eq!(e.log(), "");
}

#[test]
fn a_missing_api_file_exits_1_naming_it_and_runs_nothing() {
    let e = Env::new("nofile");
    std::fs::remove_file(e.dir.join("api/server/quadlet/acteble_api.network")).unwrap();
    let out = e.run(&["deploy", "acteble"], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("acteble_api.network"), "{out:?}");
    assert_eq!(e.log(), "");
}

#[test]
fn a_failed_copy_exits_1_and_nothing_reaches_the_server() {
    let e = Env::new("copyfail");
    let out = e.run(&["deploy", "acteble"], &[("SCP_RC", "1")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stdout).contains("deploy copy      fail ("));
    assert!(!e.log().contains("cat >"), "no secret may reach the server");
}

#[test]
fn a_failed_restart_exits_1_and_the_env_file_is_still_removed() {
    let e = Env::new("restartfail");
    let out = e.run(&["deploy", "acteble"], &[("RESTART_RC", "7")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stdout).contains("deploy restart   fail (exit 7"));
    assert!(e.log().contains("rm -f '/dev/shm/acteble-api.env'"));
}

#[test]
fn a_rollback_defaults_to_previous_and_takes_a_tag() {
    let e = Env::new("rb");
    let out = e.run(&["rollback", "acteble"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(text(&out.stdout), "rollback restart   applied\n");
    assert!(e.log().contains("acteble_api:previous"));
    assert!(!e.log().contains("scp"));
    let e = Env::new("rb-tag");
    assert_eq!(
        e.run(&["rollback", "acteble", "sha-abc"], &[])
            .status
            .code(),
        Some(0)
    );
    assert!(e.log().contains("acteble_api:sha-abc"));
}

#[test]
fn a_rollback_dry_run_runs_nothing_whatever_the_flag_position() {
    for args in [
        &["rollback", "acteble", "--dry-run"][..],
        &["rollback", "acteble", "sha-abc", "--dry-run"],
        &["rollback", "acteble", "--dry-run", "sha-abc"],
    ] {
        let e = Env::new("rb-dry");
        let out = e.run(args, &[]);
        assert_eq!(out.status.code(), Some(0), "{args:?} {out:?}");
        assert_eq!(text(&out.stdout), "rollback restart   would apply\n");
        assert_eq!(e.log(), "");
    }
}

#[test]
fn a_bad_tag_exits_1_and_the_wrong_host_exits_3() {
    let e = Env::new("bad");
    let out = e.run(&["rollback", "acteble", "x y"], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("invalid image tag"));
    script(&e.dir.join("ssh"), "echo other");
    let out = e.run(&["deploy", "acteble"], &[]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(e.log(), "");
}

#[test]
fn unknown_server_and_usage_errors_exit_2() {
    let e = Env::new("usage");
    assert_eq!(e.run(&["deploy", "nope"], &[]).status.code(), Some(2));
    assert_eq!(e.run(&["rollback", "nope"], &[]).status.code(), Some(2));
    for args in [
        &["deploy"][..],
        &["deploy", "a", "b"],
        &["deploy", "--dry-run"],
        &["deploy", "-x"],
        &["rollback"],
        &["rollback", "a", "t", "u"],
        &["rollback", "--dry-run"],
        &["rollback", "-x"],
    ] {
        let out = e.run(args, &[]);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(text(&out.stderr).contains("usage: actserve"), "{args:?}");
    }
}
