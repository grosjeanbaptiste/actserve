//! `actserve audit NAME` through the real binary, with ssh replaced by a script.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn stage(name: &str, ssh_body: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("actserve-cli-{}-{name}", std::process::id()));
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
        .env("DOTFILES_SSH", ssh)
        .output()
        .unwrap()
}

// The fake answers the hostname probe, and the audit with $AUDIT_RC (default 0).
const SSH: &str = r#"case "$*" in *"edge audit"*) echo "audit says hi"; exit "${AUDIT_RC:-0}";; *) echo host-1;; esac"#;

#[test]
fn a_conforming_audit_prints_its_output_and_exits_0() {
    let (dir, ssh) = stage("ok", SSH);
    let out = run(&dir, &ssh, &["audit", "acteble"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "audit says hi\n");
}

#[test]
fn a_deviation_exits_1() {
    let (dir, ssh) = stage("dev", &format!("AUDIT_RC=1\n{SSH}"));
    assert_eq!(
        run(&dir, &ssh, &["audit", "acteble"]).status.code(),
        Some(1)
    );
}

#[test]
fn an_unreadable_audit_exits_3_and_says_so() {
    let (dir, ssh) = stage("unr", &format!("AUDIT_RC=255\n{SSH}"));
    let out = run(&dir, &ssh, &["audit", "acteble"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stdout).contains("audit unreadable"));
}

#[test]
fn an_unknown_name_exits_2_naming_it_on_stderr() {
    let (dir, ssh) = stage("unk", SSH);
    let out = run(&dir, &ssh, &["audit", "nope"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("nope"));
}

#[test]
fn audit_without_a_name_is_a_usage_error_not_a_server_start() {
    let (dir, ssh) = stage("usage", SSH);
    let out = run(&dir, &ssh, &["audit"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: actserve audit NAME"));
}

#[test]
fn status_prints_one_line_per_server_and_exits_0_when_all_are_ok() {
    let (dir, ssh) = stage("st-ok", SSH);
    let out = run(&dir, &ssh, &["status"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "acteble: ok\n");
}

#[test]
fn status_exits_1_and_names_both_hostnames_for_another_host() {
    let (dir, ssh) = stage("st-wrong", "echo other-box");
    let out = run(&dir, &ssh, &["status", "acteble"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "acteble: wrong-host (answered other-box, expected host-1)\n"
    );
}
