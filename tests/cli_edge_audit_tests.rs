//! `actserve edge-audit NAME`, the name of `acteble edge-audit`, through the real binary.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn stage(name: &str, ssh_body: &str) -> (PathBuf, PathBuf) {
    let dir =
        std::env::temp_dir().join(format!("actserve-edge-audit-{}-{name}", std::process::id()));
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

const SSH: &str = r#"case "$*" in *"edge audit"*) echo "audit says hi"; exit "${AUDIT_RC:-0}";; *) echo host-1;; esac"#;

#[test]
fn a_conforming_edge_audit_prints_its_output_and_exits_0() {
    let (dir, ssh) = stage("ok", SSH);
    let out = run(&dir, &ssh, &["edge-audit", "acteble"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "audit says hi\n");
}

#[test]
fn a_deviation_exits_1() {
    let (dir, ssh) = stage("dev", &format!("AUDIT_RC=1\n{SSH}"));
    assert_eq!(
        run(&dir, &ssh, &["edge-audit", "acteble"]).status.code(),
        Some(1)
    );
}

#[test]
fn an_unreadable_audit_exits_3() {
    let (dir, ssh) = stage("unr", &format!("AUDIT_RC=255\n{SSH}"));
    let out = run(&dir, &ssh, &["edge-audit", "acteble"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stdout).contains("audit unreadable"));
}

#[test]
fn another_host_is_refused_before_any_audit_runs() {
    let (dir, ssh) = stage(
        "wrong",
        r#"case "$*" in *"edge audit"*) echo RAN; exit 0;; *) echo other-box;; esac"#,
    );
    let out = run(&dir, &ssh, &["edge-audit", "acteble"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("RAN"));
}

#[test]
fn an_unknown_name_exits_2_naming_it() {
    let (dir, ssh) = stage("unk", SSH);
    let out = run(&dir, &ssh, &["edge-audit", "nope"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("nope"));
}

#[test]
fn without_a_name_the_usage_names_edge_audit_and_exits_2() {
    let (dir, ssh) = stage("usage", SSH);
    let out = run(&dir, &ssh, &["edge-audit"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: actserve edge-audit NAME"));
}
