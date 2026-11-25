//! `actserve harden NAME [--dry-run]` through the real binary, ssh replaced by a script.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn stage(name: &str, ssh_body: &str, ports: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("actserve-ch-{}-{name}", std::process::id()));
    std::fs::create_dir_all(dir.join("servers/acteble")).unwrap();
    std::fs::write(dir.join("servers.txt"), "acteble|api.acteble|host-1\n").unwrap();
    std::fs::write(
        dir.join("servers/acteble/sshd.conf"),
        "PermitRootLogin no\n",
    )
    .unwrap();
    std::fs::write(dir.join("servers/acteble/ports.txt"), ports).unwrap();
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

// Hostname probe answers host-1; every check passes unless $FAIL names a command part.
const SSH: &str = r#"case "$*" in *" hostname") echo host-1; exit 0;; esac
cat > /dev/null
case "$*" in *"$FAIL"*) [ -n "$FAIL" ] && exit 1;; esac
exit 0"#;

#[test]
fn a_server_already_in_its_baseline_prints_ok_lines_and_exits_0() {
    let (dir, ssh) = stage("ok", SSH, "22/tcp\n");
    let out = run(&dir, &ssh, &["harden", "acteble"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "reach      ok\nharden sshd      ok\nharden ufw       ok\nharden fail2ban  ok\nharden upgrades  ok\n"
    );
}

#[test]
fn a_dry_run_says_would_apply_and_exits_0_without_applying() {
    let (dir, ssh) = stage("dry", &format!("FAIL=is-active\n{SSH}"), "22/tcp\n");
    let out = run(&dir, &ssh, &["harden", "acteble", "--dry-run"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("harden fail2ban  would apply\n"));
}

#[test]
fn a_failing_step_exits_1_and_prints_fail() {
    let (dir, ssh) = stage("fail", &format!("FAIL=sudo\n{SSH}"), "22/tcp\n");
    let out = run(&dir, &ssh, &["harden", "acteble"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("reach      fail ("));
}

#[test]
fn another_host_exits_3_and_names_both_hostnames() {
    let (dir, ssh) = stage("wrong", "echo other-box", "22/tcp\n");
    let out = run(&dir, &ssh, &["harden", "acteble"]);
    assert_eq!(out.status.code(), Some(3));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("other-box") && text.contains("host-1"),
        "{text}"
    );
}

#[test]
fn ports_without_ssh_are_refused_with_the_file_named_and_exit_1() {
    let (dir, ssh) = stage("ports", SSH, "80/tcp\n");
    let out = run(&dir, &ssh, &["harden", "acteble"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("ports.txt"));
}

#[test]
fn unknown_name_and_missing_name_exit_2() {
    let (dir, ssh) = stage("unk", SSH, "22/tcp\n");
    let out = run(&dir, &ssh, &["harden", "nope"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("nope"));
    let out = run(&dir, &ssh, &["harden"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("usage: actserve harden NAME [--dry-run]")
    );
}

#[test]
fn an_unknown_flag_is_a_usage_error_not_a_server_start() {
    let (dir, ssh) = stage("flag", SSH, "22/tcp\n");
    let out = run(&dir, &ssh, &["harden", "acteble", "--dry-rnu"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: actserve harden"));
}

#[test]
fn a_lone_flag_is_a_usage_error_not_an_unknown_server() {
    let (dir, ssh) = stage("lone", SSH, "22/tcp\n");
    let out = run(&dir, &ssh, &["harden", "--dry-run"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: actserve harden"));
}

// sshd drop-in differs, so it is installed over a held connection; closing it
// answers $EXIT_RC.
const SSHD_DRIFT: &str = r#"case "$*" in
  *" hostname") echo host-1; exit 0;;
  *"cmp -s"*) cat > /dev/null; exit 1;;
  *"-O exit"*) [ "$EXIT_RC" = 1 ] && { echo boom >&2; exit 1; }; exit 0;;
esac
cat > /dev/null; exit 0"#;

#[test]
fn a_held_connection_that_will_not_close_is_warned_about_on_stderr() {
    let (dir, ssh) = stage("close-bad", &format!("EXIT_RC=1\n{SSHD_DRIFT}"), "22/tcp\n");
    let out = run(&dir, &ssh, &["harden", "acteble"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("warn: closing the held connection") && err.contains("boom"),
        "{err}"
    );
}

#[test]
fn a_held_connection_that_closes_cleanly_prints_no_warning() {
    let (dir, ssh) = stage("close-ok", SSHD_DRIFT, "22/tcp\n");
    let out = run(&dir, &ssh, &["harden", "acteble"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("harden sshd      applied"));
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
}
