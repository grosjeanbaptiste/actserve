//! `actserve edge-status NAME` through the real binary: ssh and actesskey replaced by scripts.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn stage(name: &str, ssh_body: &str, key_body: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("actserve-es-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("servers.txt"), "acteble|api.acteble|host-1\n").unwrap();
    let (ssh, key) = (dir.join("ssh"), dir.join("actesskey"));
    script(&ssh, ssh_body);
    script(&key, key_body);
    (dir, ssh, key)
}

fn run(dir: &Path, ssh: &Path, key: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_actserve"))
        .args(args)
        .env("DOTFILES_STATE_ROOT", dir)
        .env("DOTFILES_SSH", ssh)
        .env("ACTSERVE_ACTESSKEY", key)
        .output()
        .unwrap()
}

/// Hostname probe answers; every other call is logged with its stdin.
const SSH: &str = r#"case "$*" in *" hostname") echo host-1; exit 0;; esac
echo "ARGS $*" >> "$0.log"
case "$*" in *"cat >"*) cat >> "$0.log";; esac
case "$*" in *"edge status"*) echo "edge: running"; exit ${RC:-0};; esac
exit 0"#;
const KEY: &str = r#"[ "$1 $2" = "get acteble-edge-prod" ] || exit 9
printf 'TOKEN=s3cr3t\n'"#;

fn log(ssh: &Path) -> String {
    std::fs::read_to_string(format!("{}.log", ssh.display())).unwrap_or_default()
}

#[test]
fn edge_status_writes_the_env_runs_the_edge_and_removes_the_env() {
    let (dir, ssh, key) = stage("ok", SSH, KEY);
    let out = run(&dir, &ssh, &key, &["edge-status", "acteble"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "edge: running\n");
    let l = log(&ssh);
    let (w, s, r) = (
        l.find("cat > '/dev/shm/acteble-edge.env'").unwrap(),
        l.find("./edge status --env-file /dev/shm/acteble-edge.env")
            .unwrap(),
        l.find("rm -f '/dev/shm/acteble-edge.env'").unwrap(),
    );
    assert!(w < s && s < r, "{l}");
    assert!(
        l.contains("TOKEN=s3cr3t"),
        "the secret travels on stdin: {l}"
    );
    assert!(
        !l.lines()
            .any(|x| x.starts_with("ARGS") && x.contains("s3cr3t"))
    );
}

#[test]
fn edge_status_exits_with_the_remote_code() {
    let (dir, ssh, key) = stage("rc", &format!("RC=6\n{SSH}"), KEY);
    assert_eq!(
        run(&dir, &ssh, &key, &["edge-status", "acteble"])
            .status
            .code(),
        Some(6)
    );
}

#[test]
fn a_secret_that_cannot_be_read_exits_1_and_runs_nothing_on_the_server() {
    let (dir, ssh, key) = stage("nokey", SSH, "echo 'locked' >&2; exit 1");
    let out = run(&dir, &ssh, &key, &["edge-status", "acteble"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("locked"),
        "{out:?}"
    );
    assert_eq!(log(&ssh), "");
}

#[test]
fn the_wrong_host_exits_3_before_the_secret_is_read() {
    let (dir, ssh, key) = stage("wrong", "echo other", "touch \"$0.ran\"; echo x");
    let out = run(&dir, &ssh, &key, &["edge-status", "acteble"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(!PathBuf::from(format!("{}.ran", key.display())).exists());
}

#[test]
fn unknown_server_exits_2_and_usage_errors_exit_2() {
    let (dir, ssh, key) = stage("usage", SSH, KEY);
    assert_eq!(
        run(&dir, &ssh, &key, &["edge-status", "nope"])
            .status
            .code(),
        Some(2)
    );
    for args in [
        &["edge-status"][..],
        &["edge-status", "a", "b"],
        &["edge-status", "-x"],
    ] {
        let out = run(&dir, &ssh, &key, args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("usage: actserve edge-status"));
    }
}
