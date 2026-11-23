//! `ActesskeySecretStore` against a fake `actesskey` script.
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use actserve::adapters::outbound::system::actesskey_secret_store::ActesskeySecretStore;
use actserve::domain::errors::DomainError;
use actserve::ports::outbound::secret_store::SecretStorePort;

fn fake(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("actserve-sec-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("actesskey");
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

#[tokio::test]
async fn it_asks_actesskey_get_and_returns_the_output_verbatim() {
    let log_dir = std::env::temp_dir().join(format!("actserve-sec-args-{}", std::process::id()));
    let bin = fake(
        "ok",
        &format!(
            "echo \"$*\" > '{}'; printf 'A=1\\nB=2\\n'",
            log_dir.display()
        ),
    );
    let v = ActesskeySecretStore::new(bin)
        .get("my-secret")
        .await
        .unwrap();
    assert_eq!(v.expose(), "A=1\nB=2\n");
    assert_eq!(std::fs::read_to_string(log_dir).unwrap(), "get my-secret\n");
}

#[tokio::test]
async fn a_failing_actesskey_is_an_error_with_its_message_and_no_value() {
    let bin = fake("fail", "echo 'no such secret' >&2; echo LEAK; exit 3");
    let e = ActesskeySecretStore::new(bin)
        .get("my-secret")
        .await
        .unwrap_err();
    let DomainError::ExecutionFailed { reason, details } = &e else {
        panic!("{e:?}")
    };
    assert!(
        reason.contains("my-secret") && reason.contains('3'),
        "{reason}"
    );
    assert!(details.as_deref().unwrap().contains("no such secret"));
    assert!(!format!("{e:?}").contains("LEAK"));
}

#[tokio::test]
async fn an_empty_secret_is_an_error() {
    let bin = fake("empty", "exit 0");
    let e = ActesskeySecretStore::new(bin)
        .get("my-secret")
        .await
        .unwrap_err();
    assert!(
        e.to_string().contains("empty") || format!("{e:?}").contains("empty"),
        "{e:?}"
    );
}

#[tokio::test]
async fn a_missing_binary_is_an_error_naming_it() {
    let e = ActesskeySecretStore::new("/nonexistent/actesskey")
        .get("my-secret")
        .await
        .unwrap_err();
    assert!(format!("{e:?}").contains("/nonexistent/actesskey"), "{e:?}");
}

#[tokio::test]
async fn a_name_that_is_not_a_secret_name_never_reaches_actesskey() {
    let bin = fake("never", "touch \"$0.ran\"");
    for bad in ["", "-x", "A b", "a;b"] {
        let e = ActesskeySecretStore::new(bin.clone())
            .get(bad)
            .await
            .unwrap_err();
        assert!(matches!(e, DomainError::ValidationError(_)), "{bad:?}");
    }
    assert!(!PathBuf::from(format!("{}.ran", bin.display())).exists());
}

#[tokio::test]
async fn a_secret_that_is_not_utf8_is_an_error_not_a_silent_alteration() {
    let bin = fake("bin", "printf '\\377\\376'");
    let e = ActesskeySecretStore::new(bin)
        .get("my-secret")
        .await
        .unwrap_err();
    assert!(format!("{e:?}").contains("UTF-8"), "{e:?}");
}
