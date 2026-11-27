//! `actserve ps NAME STACK` and `actserve logs NAME STACK [SERVICE]` through the real binary, ssh replaced by a script.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn stage(name: &str, ssh_body: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("actserve-obs-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("servers.txt"), "acteble|api.acteble|host-1\n").unwrap();
    let ssh = dir.join("ssh");
    std::fs::write(&ssh, format!("#!/bin/sh\n{ssh_body}\n")).unwrap();
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, ssh)
}

fn run(dir: &Path, ssh: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_actserve"))
        .args(args)
        .env("ACTCONF_STATE_ROOT", dir)
        .env("ACTSERVE_SSH", ssh)
        .output()
        .unwrap()
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// Answers the hostname probe, logs every other call's arguments to `$0.log`.
const SSH: &str = r#"case "$*" in *" hostname") echo host-1; exit 0;; esac
echo "$*" >> "$0.log"
echo "service-line"
echo "remote-warning" >&2
exit ${RC:-0}"#;

fn calls(ssh: &Path) -> String {
    std::fs::read_to_string(format!("{}.log", ssh.display())).unwrap_or_default()
}

#[test]
fn ps_prints_what_the_server_answers() {
    let (dir, ssh) = stage("ps", SSH);
    let out = run(&dir, &ssh, &["ps", "acteble", "cdn"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(text(&out.stdout), "service-line\n");
    assert_eq!(text(&out.stderr), "remote-warning\n");
    assert!(
        calls(&ssh).contains("api.acteble podman ps --filter name=acteble_cdn"),
        "{}",
        calls(&ssh)
    );
}

#[test]
fn ps_exits_with_the_remote_code() {
    let (dir, ssh) = stage("ps-rc", &format!("RC=5\n{SSH}"));
    assert_eq!(
        run(&dir, &ssh, &["ps", "acteble", "api"]).status.code(),
        Some(5)
    );
}

#[test]
fn logs_stream_through_a_terminal_with_the_default_service() {
    let (dir, ssh) = stage("logs", SSH);
    let out = run(&dir, &ssh, &["logs", "acteble", "cdn"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(text(&out.stdout), "service-line\n");
    let c = calls(&ssh);
    assert!(c.starts_with("-t "), "{c}");
    assert!(
        c.contains("journalctl --user -u acteble_cdn-checkpoint.service -f -n 100"),
        "{c}"
    );
}

#[test]
fn logs_take_a_service_and_exit_with_the_ssh_code() {
    let (dir, ssh) = stage("logs-svc", &format!("RC=130\n{SSH}"));
    let out = run(&dir, &ssh, &["logs", "acteble", "api", "db"]);
    assert_eq!(out.status.code(), Some(130));
    assert!(
        calls(&ssh).contains("acteble_api-db.service -f -n 100"),
        "{}",
        calls(&ssh)
    );
}

#[test]
fn the_wrong_host_exits_3_and_runs_nothing_else() {
    let (dir, ssh) = stage("wrong", "echo other");
    let out = run(&dir, &ssh, &["ps", "acteble", "api"]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(
        text(&out.stdout),
        "acteble: wrong-host (answered other, expected host-1)\n"
    );
    assert_eq!(calls(&ssh), "");
}

#[test]
fn an_unknown_server_exits_2() {
    let (dir, ssh) = stage("unknown", SSH);
    let out = run(&dir, &ssh, &["ps", "nope", "api"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("unknown server: nope"));
}

#[test]
fn usage_errors_exit_2() {
    let (dir, ssh) = stage("usage", SSH);
    for args in [
        &["ps"][..],
        &["ps", "acteble"],
        &["ps", "acteble", "api", "extra"],
        &["logs", "acteble"],
        &["logs", "acteble", "api", "db", "extra"],
        &["ps", "-x", "api"],
    ] {
        let out = run(&dir, &ssh, args);
        assert_eq!(out.status.code(), Some(2), "{args:?} {out:?}");
        assert!(text(&out.stderr).contains("usage: actserve"), "{args:?}");
    }
}

#[test]
fn an_unknown_stack_or_a_bad_service_exits_1_saying_why() {
    let (dir, ssh) = stage("bad", SSH);
    let out = run(&dir, &ssh, &["ps", "acteble", "nope"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("unknown stack"), "{out:?}");
    let out = run(&dir, &ssh, &["logs", "acteble", "api", "a b"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("invalid service"), "{out:?}");
    assert_eq!(calls(&ssh), "");
}

#[test]
fn resources_prints_what_the_server_answers_and_changes_nothing() {
    let (dir, ssh) = stage("resources", SSH);
    let out = run(&dir, &ssh, &["resources", "acteble"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(text(&out.stdout), "service-line\n");
    let seen = calls(&ssh);
    assert!(
        seen.contains("api.acteble ") && seen.contains("free -m"),
        "{seen}"
    );
    assert!(!seen.contains("sudo"), "{seen}");
}

#[test]
fn resources_usage_errors_exit_2() {
    let (dir, ssh) = stage("resources-usage", SSH);
    for args in [
        &["resources"][..],
        &["resources", "a", "b"],
        &["resources", "-x"],
    ] {
        let out = run(&dir, &ssh, args);
        assert_eq!(out.status.code(), Some(2), "{args:?} {out:?}");
        assert!(
            text(&out.stderr).contains("usage: actserve resources NAME"),
            "{args:?}"
        );
    }
}

#[test]
fn ps_edge_lists_the_caddy_container() {
    let (dir, ssh) = stage("ps-edge", SSH);
    let out = run(&dir, &ssh, &["ps", "acteble", "edge"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        calls(&ssh).contains("podman ps --filter name=acteble_edge"),
        "{}",
        calls(&ssh)
    );
}

#[test]
fn logs_edge_follow_the_caddy_unit_by_default() {
    let (dir, ssh) = stage("logs-edge", SSH);
    let out = run(&dir, &ssh, &["logs", "acteble", "edge"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let c = calls(&ssh);
    assert!(
        c.contains("journalctl --user -u acteble_edge-caddy.service -f -n 100"),
        "{c}"
    );
}
