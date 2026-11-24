//! `ScpRemoteCopy` against a fake `scp` script.
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use actserve::adapters::outbound::system::scp_remote_copy::ScpRemoteCopy;
use actserve::ports::outbound::remote_copy::RemoteCopyPort;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("actserve-scp-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn fake(d: &std::path::Path, body: &str) -> PathBuf {
    let p = d.join("scp");
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

#[tokio::test]
async fn it_copies_the_files_to_the_remote_directory_with_options_ended() {
    let d = dir("ok");
    let scp = fake(&d, "echo \"$*\" > \"$0.args\"");
    let out = ScpRemoteCopy::new(&scp)
        .copy(
            "api.acteble",
            &["/a/x.yml".into(), "/a/Caddyfile".into()],
            "/opt/x",
        )
        .await
        .unwrap();
    assert_eq!(out.code, Some(0));
    assert_eq!(
        std::fs::read_to_string(format!("{}.args", scp.display())).unwrap(),
        "-o BatchMode=yes -o ConnectTimeout=10 -r -- /a/x.yml /a/Caddyfile api.acteble:/opt/x/\n"
    );
}

#[tokio::test]
async fn a_failing_scp_is_a_code_with_its_stderr() {
    let d = dir("fail");
    let scp = fake(&d, "echo 'denied' >&2; exit 4");
    let out = ScpRemoteCopy::new(&scp)
        .copy("t", &["/a/x".into()], "/opt/x")
        .await
        .unwrap();
    assert_eq!(out.code, Some(4));
    assert_eq!(out.stderr.trim(), "denied");
}

#[tokio::test]
async fn a_missing_binary_is_an_error_naming_it() {
    let e = ScpRemoteCopy::new("/nonexistent/scp")
        .copy("t", &["/a/x".into()], "/opt/x")
        .await
        .unwrap_err();
    assert!(format!("{e:?}").contains("/nonexistent/scp"), "{e:?}");
}

#[tokio::test]
async fn missing_lists_only_the_absent_files() {
    let d = dir("missing");
    let present = d.join("here");
    std::fs::write(&present, "x").unwrap();
    let absent = d.join("gone");
    let got = ScpRemoteCopy::new("scp")
        .missing(&[present, absent.clone()])
        .await;
    assert_eq!(got, [absent]);
}
