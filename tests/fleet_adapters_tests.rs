use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use actserve::adapters::outbound::system::{
    file_inventory::FileServerInventory, ssh_hostname_probe::SshHostnameProbe,
};
use actserve::domain::errors::DomainError;
use actserve::ports::outbound::{
    hostname_probe::HostnameProbePort, server_inventory::ServerInventoryPort,
};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("actserve-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fake_ssh(dir: &std::path::Path, body: &str) -> PathBuf {
    let path = dir.join("ssh");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[tokio::test]
async fn file_inventory_reads_and_parses_the_file() {
    let dir = scratch("inv-ok");
    let file = dir.join("servers.txt");
    std::fs::write(&file, "# c\nacteble|api.acteble|cloud-1\n").unwrap();
    let hosts = FileServerInventory::new(&file).hosts().await.unwrap();
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].name(), "acteble");
}

#[tokio::test]
async fn file_inventory_missing_file_is_an_error_naming_the_path() {
    let path = scratch("inv-missing").join("nope.txt");
    let err = FileServerInventory::new(&path).hosts().await.unwrap_err();
    assert!(
        matches!(&err, DomainError::ExecutionFailed { reason, .. } if reason.contains("nope.txt")),
        "{err:?}"
    );
}

#[tokio::test]
async fn file_inventory_malformed_file_is_a_validation_error() {
    let dir = scratch("inv-bad");
    let file = dir.join("servers.txt");
    std::fs::write(&file, "only|two\n").unwrap();
    let err = FileServerInventory::new(&file).hosts().await.unwrap_err();
    assert!(matches!(err, DomainError::ValidationError(_)));
}

#[tokio::test]
async fn probe_returns_trimmed_stdout_and_passes_target_to_ssh() {
    let dir = scratch("probe-ok");
    let ssh = fake_ssh(
        &dir,
        r#"case "$*" in *"BatchMode=yes"*"my-target hostname") echo " host-1 ";; *) echo "bad args: $*" >&2; exit 9;; esac"#,
    );
    let got = SshHostnameProbe::new(ssh)
        .hostname("my-target")
        .await
        .unwrap();
    assert_eq!(got, "host-1");
}

#[tokio::test]
async fn probe_nonzero_exit_carries_stderr() {
    let dir = scratch("probe-fail");
    let ssh = fake_ssh(&dir, "echo 'Connection refused' >&2; exit 255");
    let err = SshHostnameProbe::new(ssh).hostname("t").await.unwrap_err();
    assert!(
        matches!(&err, DomainError::ExecutionFailed { details: Some(d), .. } if d.contains("Connection refused")),
        "{err:?}"
    );
}

#[tokio::test]
async fn probe_missing_binary_is_an_error_not_a_default() {
    let missing = scratch("probe-missing").join("no-such-ssh");
    let err = SshHostnameProbe::new(missing)
        .hostname("t")
        .await
        .unwrap_err();
    assert!(matches!(err, DomainError::ExecutionFailed { .. }));
}

#[tokio::test]
async fn probe_empty_answer_is_an_error() {
    let dir = scratch("probe-empty");
    let ssh = fake_ssh(&dir, "exit 0");
    assert!(SshHostnameProbe::new(ssh).hostname("t").await.is_err());
}

#[tokio::test]
async fn probe_ends_the_options_before_the_target() {
    let dir = scratch("probe-dashes");
    let ssh = fake_ssh(
        &dir,
        r#"case "$*" in *" -- -oProxyCommand=x hostname") echo h;; *) echo "bad args: $*" >&2; exit 9;; esac"#,
    );
    let got = SshHostnameProbe::new(ssh)
        .hostname("-oProxyCommand=x")
        .await
        .unwrap();
    assert_eq!(got, "h");
}

#[tokio::test]
async fn probe_keeps_the_first_line_and_drops_control_characters() {
    let dir = scratch("probe-clean");
    let ssh = fake_ssh(&dir, r"printf 'host-1\033[31m\nsecond\n'");
    let got = SshHostnameProbe::new(ssh).hostname("t").await.unwrap();
    assert_eq!(got, "host-1[31m");
}

#[tokio::test]
async fn probe_answer_made_only_of_control_characters_is_an_error() {
    let dir = scratch("probe-ctrl");
    let ssh = fake_ssh(&dir, r"printf '\033\r\n'");
    assert!(SshHostnameProbe::new(ssh).hostname("t").await.is_err());
}
