use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use actserve::adapters::outbound::system::{
    file_declared_data::FileDeclaredData, ssh_remote_shell::SshRemoteShell,
    ssh_sshd_installer::SshSshdInstaller,
};
use actserve::domain::{errors::DomainError, harden::SshdOutcome};
use actserve::ports::outbound::{
    declared_data::DeclaredDataPort, remote_shell::RemoteShellPort,
    sshd_installer::SshdInstallerPort,
};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("actserve-h-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fake_ssh(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("ssh");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[tokio::test]
async fn the_shell_passes_target_and_command_after_double_dash_and_feeds_stdin() {
    let dir = scratch("sh-ok");
    let ssh = fake_ssh(
        &dir,
        r#"case "$*" in *"BatchMode=yes"*" -- tgt echo hi") cat; echo out; echo err >&2; exit 4;; *) echo "bad: $*" >&2; exit 9;; esac"#,
    );
    let out = SshRemoteShell::new(ssh)
        .run("tgt", "echo hi", Some("in\n"))
        .await
        .unwrap();
    assert_eq!(out.code, Some(4));
    assert_eq!(out.stdout, "in\nout\n");
    assert_eq!(out.stderr, "err\n");
}

#[tokio::test]
async fn the_shell_without_stdin_does_not_hang_and_a_missing_ssh_is_an_error() {
    let dir = scratch("sh-nostdin");
    let ssh = fake_ssh(&dir, "cat; exit 0");
    let out = SshRemoteShell::new(ssh).run("t", "c", None).await.unwrap();
    assert_eq!((out.code, out.stdout.as_str()), (Some(0), ""));
    let err = SshRemoteShell::new(dir.join("nope"))
        .run("t", "c", None)
        .await
        .unwrap_err();
    assert!(matches!(err, DomainError::ExecutionFailed { .. }));
}

#[tokio::test]
async fn the_shell_reports_a_signal_as_no_code() {
    let dir = scratch("sh-sig");
    let ssh = fake_ssh(&dir, "kill -9 $$");
    let out = SshRemoteShell::new(ssh).run("t", "c", None).await.unwrap();
    assert_eq!(out.code, None);
}

// Logs every call to $LOG; each phase answers with its own exit code.
const SSHD_SSH: &str = r#"echo "$*" >> "$LOG"
case "$*" in
  *"ControlMaster=yes"*) exit "${MASTER_RC:-0}";;
  *"cat > "*) cat > "$LOG.stdin"; echo "install failed" >&2; exit "${INSTALL_RC:-0}";;
  *"ControlPath=none"*) exit "${FRESH_RC:-0}";;
  *": rollback"*) exit 0;;
  *"-O exit"*) exit "${EXIT_RC:-0}";;
esac
exit 0"#;

async fn install(
    name: &str,
    envs: &[(&str, &str)],
) -> (Result<SshdOutcome, DomainError>, String, PathBuf) {
    let dir = scratch(name);
    let log = dir.join("log");
    std::fs::write(&log, "").unwrap();
    let mut prefix = format!("LOG='{}'\n", log.display());
    for (k, v) in envs {
        prefix.push_str(&format!("{k}={v}\n"));
    }
    let ssh = fake_ssh(&dir, &format!("{prefix}{SSHD_SSH}"));
    let r = SshSshdInstaller::new(ssh)
        .install("tgt", "PermitRootLogin no\n")
        .await;
    (r, std::fs::read_to_string(&log).unwrap(), log)
}

#[tokio::test]
async fn a_clean_install_goes_through_one_held_connection_and_a_fresh_proof() {
    let (r, log, path) = install("ssh-ok", &[]).await;
    assert_eq!(r.unwrap(), SshdOutcome::Installed);
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 4, "{log}");
    assert!(lines[0].contains("ControlMaster=yes") && lines[0].contains("-fN -- tgt"));
    assert!(lines[1].contains("sshd -t && systemctl reload ssh") && lines[1].contains("-- tgt"));
    assert!(lines[2].contains("ControlPath=none") && lines[2].ends_with("-- tgt true"));
    assert!(lines[3].contains("-O exit"));
    let stdin = std::fs::read_to_string(format!("{}.stdin", path.display())).unwrap();
    assert_eq!(stdin, "PermitRootLogin no\n");
}

#[tokio::test]
async fn a_failed_fresh_connection_rolls_back_through_the_held_one() {
    let (r, log, _) = install("ssh-fresh", &[("FRESH_RC", "255")]).await;
    assert_eq!(r.unwrap(), SshdOutcome::RolledBack);
    assert!(log.contains(": rollback"), "{log}");
    assert!(log.lines().last().unwrap().contains("-O exit"));
}

#[tokio::test]
async fn a_failing_install_script_is_a_failure_with_its_stderr_and_no_fresh_proof() {
    let (r, log, _) = install("ssh-inst", &[("INSTALL_RC", "1")]).await;
    assert!(
        matches!(&r, Ok(SshdOutcome::Failed { reason }) if reason.contains("install failed")),
        "{r:?}"
    );
    assert!(!log.contains("ControlPath=none"), "{log}");
    assert!(log.lines().last().unwrap().contains("-O exit"));
}

#[tokio::test]
async fn a_master_that_cannot_open_is_a_failure_and_nothing_is_installed() {
    let (r, log, _) = install("ssh-master", &[("MASTER_RC", "255")]).await;
    assert!(
        matches!(&r, Ok(SshdOutcome::Failed { reason }) if reason.contains("connection")),
        "{r:?}"
    );
    assert!(!log.contains("cat > "), "{log}");
    assert_eq!(log.lines().count(), 2, "{log}"); // open, then the best-effort close
}

#[tokio::test]
async fn a_missing_ssh_binary_is_an_error() {
    let dir = scratch("ssh-missing");
    let err = SshSshdInstaller::new(dir.join("nope"))
        .install("t", "x")
        .await
        .unwrap_err();
    assert!(matches!(err, DomainError::ExecutionFailed { .. }));
}

#[tokio::test]
async fn declared_data_reads_servers_name_file_and_names_the_path_when_missing() {
    let dir = scratch("data");
    std::fs::create_dir_all(dir.join("servers/acteble")).unwrap();
    std::fs::write(dir.join("servers/acteble/ports.txt"), "22/tcp\n").unwrap();
    let data = FileDeclaredData::new(&dir);
    assert_eq!(data.read("acteble", "ports.txt").await.unwrap(), "22/tcp\n");
    let err = data.read("acteble", "sshd.conf").await.unwrap_err();
    assert!(
        matches!(&err, DomainError::ExecutionFailed { reason, .. } if reason.contains("sshd.conf")),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_command_that_exits_without_reading_a_large_stdin_answers_with_its_code() {
    let dir = scratch("sh-pipe");
    let ssh = fake_ssh(&dir, "exit 3");
    let big = "x".repeat(4 * 1024 * 1024);
    let out = SshRemoteShell::new(ssh)
        .run("t", "c", Some(&big))
        .await
        .unwrap();
    assert_eq!(out.code, Some(3));
}
