//! `SshRemoteDownload` against a script standing in for ssh.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use actserve::adapters::outbound::system::ssh_remote_download::SshRemoteDownload;
use actserve::ports::outbound::remote_download::RemoteDownloadPort;

fn stage(name: &str, body: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("actserve-dl-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ssh = dir.join("ssh");
    std::fs::write(&ssh, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, ssh)
}
fn mode(p: &Path) -> u32 {
    std::fs::metadata(p).unwrap().permissions().mode() & 0o777
}

#[tokio::test]
async fn the_bytes_are_written_untouched_to_a_private_file_in_a_private_directory() {
    let (dir, ssh) = stage("ok", r#"printf 'AB\377\000CD'"#);
    let dest = dir.join("out/deep/a.tar.gz");
    let out = SshRemoteDownload::new(ssh)
        .download("t", "cmd", &dest)
        .await
        .unwrap();
    assert_eq!(out.code, Some(0));
    assert_eq!(std::fs::read(&dest).unwrap(), b"AB\xff\x00CD");
    assert_eq!(mode(&dest), 0o600);
    assert_eq!(mode(dest.parent().unwrap()), 0o700);
}

#[tokio::test]
async fn an_existing_wider_directory_is_tightened_to_700() {
    let (dir, ssh) = stage("tight", "printf x");
    let out_dir = dir.join("wide");
    std::fs::create_dir_all(&out_dir).unwrap();
    std::fs::set_permissions(&out_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    SshRemoteDownload::new(ssh)
        .download("t", "c", &out_dir.join("f"))
        .await
        .unwrap();
    assert_eq!(mode(&out_dir), 0o700);
}

#[tokio::test]
async fn ssh_receives_batch_mode_the_target_and_the_command() {
    let (dir, ssh) = stage(
        "args",
        r#"printf '%s\n' "$*" > "$(dirname "$0")/args"; printf x"#,
    );
    SshRemoteDownload::new(ssh)
        .download("tgt", "do it", &dir.join("f"))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("args")).unwrap(),
        "-o BatchMode=yes -o ConnectTimeout=10 -- tgt do it\n"
    );
}

#[tokio::test]
async fn a_failing_command_leaves_no_file_and_returns_its_code_and_stderr() {
    let (dir, ssh) = stage("fail", "printf partial; echo boom >&2; exit 7");
    let dest = dir.join("f");
    let out = SshRemoteDownload::new(ssh)
        .download("t", "c", &dest)
        .await
        .unwrap();
    assert_eq!(out.code, Some(7));
    assert_eq!(out.stderr.trim(), "boom");
    assert!(!dest.exists(), "a partial archive must not stay behind");
}

#[tokio::test]
async fn a_missing_ssh_is_an_error_and_leaves_no_file() {
    let (dir, _) = stage("nossh", "true");
    let dest = dir.join("f");
    let e = SshRemoteDownload::new(dir.join("absent"))
        .download("t", "c", &dest)
        .await
        .unwrap_err();
    assert!(
        e.to_string().contains("absent") || format!("{e:?}").contains("absent"),
        "{e:?}"
    );
    assert!(!dest.exists());
}

#[tokio::test]
async fn an_existing_file_is_never_overwritten() {
    let (dir, ssh) = stage("exists", "printf new");
    let dest = dir.join("f");
    std::fs::write(&dest, "old").unwrap();
    let e = SshRemoteDownload::new(ssh)
        .download("t", "c", &dest)
        .await
        .unwrap_err();
    assert!(format!("{e:?}").contains("f"), "{e:?}");
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "old");
}
