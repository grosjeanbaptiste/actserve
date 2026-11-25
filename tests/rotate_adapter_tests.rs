//! The rotation adapters against fake `actesskey` and `machine` scripts and fake entropy files.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use actserve::adapters::outbound::system::{
    actesskey_secret_writer::ActesskeySecretWriter, machine_state_persist::MachineStatePersist,
    os_random::OsRandom,
};
use actserve::domain::{errors::DomainError, secret::SecretValue};
use actserve::ports::outbound::{
    random::RandomPort, secret_writer::SecretWriterPort, state_persist::StatePersistPort,
};

fn stage(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("actserve-rot-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn fake(dir: &Path, body: &str) -> PathBuf {
    let p = dir.join("tool");
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}
fn pairs(list: &[(&str, &str)]) -> Vec<(String, SecretValue)> {
    list.iter()
        .map(|(k, v)| (k.to_string(), SecretValue::new(v.to_string())))
        .collect()
}

#[tokio::test]
async fn the_writer_runs_kv_set_and_feeds_key_tab_value_lines_on_stdin() {
    let dir = stage("w-ok");
    let bin = fake(
        &dir,
        &format!("echo \"$*\" > '{0}/args'; cat > '{0}/stdin'", dir.display()),
    );
    ActesskeySecretWriter::new(bin)
        .set_values(
            "acteble-api-prod",
            &pairs(&[("A_KEY", "abc+/=="), ("B_KEY", "0123")]),
        )
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("args")).unwrap(),
        "kv-set acteble-api-prod\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("stdin")).unwrap(),
        "A_KEY\tabc+/==\nB_KEY\t0123\n"
    );
}

#[tokio::test]
async fn a_failing_writer_says_why_and_never_shows_a_value() {
    let dir = stage("w-fail");
    let bin = fake(&dir, "cat >&2; echo 'store locked' >&2; exit 4");
    let e = ActesskeySecretWriter::new(bin)
        .set_values("acteble-api-prod", &pairs(&[("A_KEY", "s3cr3t-value")]))
        .await
        .unwrap_err();
    let DomainError::ExecutionFailed { reason, details } = &e else {
        panic!("{e:?}")
    };
    assert!(reason.contains("kv-set acteble-api-prod") && reason.contains('4'));
    assert!(details.as_deref().unwrap().contains("store locked"));
    assert!(!format!("{e:?}").contains("s3cr3t-value"), "{e:?}");
}

#[tokio::test]
async fn the_writer_refuses_what_kv_set_could_misread_before_running_anything() {
    let dir = stage("w-bad");
    let bin = fake(&dir, &format!("touch '{}/ran'", dir.display()));
    let writer = ActesskeySecretWriter::new(bin);
    let cases: [(&str, &[(&str, &str)]); 7] = [
        ("Bad Name", &[("A", "1")]),
        ("", &[("A", "1")]),
        ("ok-name", &[]),
        ("ok-name", &[("", "1")]),
        ("ok-name", &[("A=B", "1")]),
        ("ok-name", &[("A", "x\ty")]),
        ("ok-name", &[("A", "x\ny")]),
    ];
    for (name, list) in cases {
        let e = writer.set_values(name, &pairs(list)).await.unwrap_err();
        assert!(matches!(e, DomainError::ValidationError(_)), "{name} {e:?}");
    }
    let e = writer
        .set_values("ok-name", &pairs(&[("A\tB", "1")]))
        .await
        .unwrap_err();
    assert!(matches!(e, DomainError::ValidationError(_)));
    assert!(!dir.join("ran").exists());
}

#[tokio::test]
async fn a_writer_binary_that_cannot_run_is_an_error_naming_it() {
    let e = ActesskeySecretWriter::new("/nonexistent/actesskey")
        .set_values("ok-name", &pairs(&[("A", "1")]))
        .await
        .unwrap_err();
    assert!(
        matches!(&e, DomainError::ExecutionFailed { reason, .. } if reason.contains("/nonexistent/actesskey")),
        "{e:?}"
    );
}

#[tokio::test]
async fn persist_runs_machine_autocommit() {
    let dir = stage("p-ok");
    let bin = fake(&dir, &format!("echo \"$*\" > '{}/args'", dir.display()));
    MachineStatePersist::new(bin).persist().await.unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("args")).unwrap(),
        "autocommit\n"
    );
}

#[tokio::test]
async fn a_failed_persist_carries_the_exit_code_and_stderr() {
    let dir = stage("p-fail");
    let bin = fake(&dir, "echo 'push rejected' >&2; exit 7");
    let e = MachineStatePersist::new(bin).persist().await.unwrap_err();
    let DomainError::ExecutionFailed { reason, details } = &e else {
        panic!("{e:?}")
    };
    assert!(reason.contains("machine autocommit") && reason.contains('7'));
    assert!(details.as_deref().unwrap().contains("push rejected"));
}

#[tokio::test]
async fn a_persist_binary_that_cannot_run_is_an_error_naming_it() {
    let e = MachineStatePersist::new("/nonexistent/machine")
        .persist()
        .await
        .unwrap_err();
    assert!(
        matches!(&e, DomainError::ExecutionFailed { reason, .. } if reason.contains("/nonexistent/machine")),
        "{e:?}"
    );
}

#[test]
fn the_system_entropy_gives_the_bytes_asked_and_never_the_same_twice() {
    let r = OsRandom::system();
    let a = r.bytes(32).unwrap();
    let b = r.bytes(32).unwrap();
    assert_eq!((a.len(), b.len()), (32, 32));
    assert_ne!(a, b);
    assert!(r.bytes(0).unwrap().is_empty());
    assert_eq!(r.bytes(64).unwrap().len(), 64);
}

#[test]
fn entropy_is_read_from_the_source_given() {
    let dir = stage("r-src");
    let src = dir.join("src");
    std::fs::write(&src, [7u8; 16]).unwrap();
    assert_eq!(OsRandom::from_source(&src).bytes(8).unwrap(), vec![7u8; 8]);
}

#[test]
fn a_missing_or_short_source_is_an_error_never_a_weaker_value() {
    let dir = stage("r-bad");
    let src = dir.join("src");
    std::fs::write(&src, [7u8; 4]).unwrap();
    for source in [dir.join("absent"), src] {
        let e = OsRandom::from_source(&source).bytes(8).unwrap_err();
        assert!(
            matches!(&e, DomainError::ExecutionFailed { reason, .. } if reason.contains("random")),
            "{e:?}"
        );
    }
}
