//! A script still open for writing makes exec fail with ETXTBSY ("Text file
//! busy"): a fork in another thread holds a copy of the descriptor until its
//! own exec. The adapters wait it out instead of reporting a false failure.
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use actserve::adapters::outbound::system::{
    ssh_edge_audit::SshEdgeAudit, ssh_hostname_probe::SshHostnameProbe,
    ssh_remote_shell::SshRemoteShell,
};
use actserve::ports::outbound::{
    edge_audit::EdgeAuditPort, hostname_probe::HostnameProbePort, remote_shell::RemoteShellPort,
};

/// An ssh stand-in kept open for writing for 150 ms.
fn busy_ssh(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("actserve-busy-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ssh");
    std::fs::write(&path, "#!/bin/sh\necho host-1\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let held = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        drop(held);
    });
    path
}

#[tokio::test]
async fn the_hostname_probe_waits_for_a_busy_script() {
    let ssh = busy_ssh("probe");
    assert_eq!(
        SshHostnameProbe::new(ssh).hostname("t").await.unwrap(),
        "host-1"
    );
}

#[tokio::test]
async fn the_edge_audit_waits_for_a_busy_script() {
    let ssh = busy_ssh("audit");
    assert!(SshEdgeAudit::new(ssh).audit("t").await.is_ok());
}

#[tokio::test]
async fn the_remote_shell_waits_for_a_busy_script() {
    let ssh = busy_ssh("shell");
    let out = SshRemoteShell::new(ssh).run("t", "c", None).await.unwrap();
    assert_eq!(out.stdout, "host-1\n");
}
