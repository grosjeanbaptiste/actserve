//! `with_remote_env`: the secret goes over stdin to a tmpfs file that is removed whatever happens.
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::application::remote_env::with_remote_env;
use actserve::domain::{errors::DomainError, secret::SecretValue};
use actserve::ports::outbound::{
    remote_shell::{RemoteOutput, RemoteShellPort},
    secret_store::SecretStorePort,
};

const PATH: &str = "/dev/shm/x.env";
const VALUE: &str = "TOP-SECRET-VALUE";

#[derive(Default)]
struct Log(Mutex<Vec<(String, Option<String>)>>);

struct Shell {
    log: Arc<Log>,
    /// exit code per command prefix, default 0
    codes: Vec<(&'static str, i32)>,
}
#[async_trait]
impl RemoteShellPort for Shell {
    async fn run(&self, _t: &str, c: &str, i: Option<&str>) -> Result<RemoteOutput, DomainError> {
        self.log
            .0
            .lock()
            .unwrap()
            .push((c.into(), i.map(String::from)));
        let code = self
            .codes
            .iter()
            .find(|(p, _)| c.starts_with(p))
            .map_or(0, |(_, r)| *r);
        Ok(RemoteOutput {
            code: Some(code),
            stdout: format!("out:{c}"),
            stderr: String::new(),
        })
    }
}
struct Store(Result<&'static str, &'static str>);
#[async_trait]
impl SecretStorePort for Store {
    async fn get(&self, name: &str) -> Result<SecretValue, DomainError> {
        assert_eq!(name, "my-secret");
        self.0
            .map(|v| SecretValue::new(v.into()))
            .map_err(|m| DomainError::ExecutionFailed {
                reason: m.into(),
                details: None,
            })
    }
}

fn setup(codes: Vec<(&'static str, i32)>, store: Store) -> (Arc<Log>, Shell, Store) {
    let log = Arc::new(Log::default());
    (log.clone(), Shell { log, codes }, store)
}
fn commands(log: &Log) -> Vec<String> {
    log.0
        .lock()
        .unwrap()
        .iter()
        .map(|(c, _)| c.clone())
        .collect()
}

#[tokio::test]
async fn the_secret_is_written_the_command_runs_and_the_file_is_removed() {
    let (log, shell, store) = setup(vec![], Store(Ok(VALUE)));
    let out = with_remote_env(&shell, &store, "t", "my-secret", PATH, "do-it")
        .await
        .unwrap();
    assert_eq!(out.stdout, "out:do-it");
    assert_eq!(
        commands(&log),
        [
            "umask 077 && cat > '/dev/shm/x.env'",
            "do-it",
            "rm -f '/dev/shm/x.env'"
        ]
    );
    assert_eq!(log.0.lock().unwrap()[0].1.as_deref(), Some(VALUE));
}

#[tokio::test]
async fn the_secret_is_never_part_of_a_command() {
    let (log, shell, store) = setup(vec![], Store(Ok(VALUE)));
    with_remote_env(&shell, &store, "t", "my-secret", PATH, "do-it")
        .await
        .unwrap();
    for (c, i) in log.0.lock().unwrap().iter().skip(1) {
        assert!(!c.contains(VALUE) && i.is_none(), "{c}");
    }
}

#[tokio::test]
async fn a_failing_command_still_removes_the_file_and_returns_its_code() {
    let (log, shell, store) = setup(vec![("do-it", 4)], Store(Ok(VALUE)));
    let out = with_remote_env(&shell, &store, "t", "my-secret", PATH, "do-it")
        .await
        .unwrap();
    assert_eq!(out.code, Some(4));
    assert_eq!(commands(&log).last().unwrap(), "rm -f '/dev/shm/x.env'");
}

#[tokio::test]
async fn a_failed_upload_removes_the_file_and_never_runs_the_command() {
    let (log, shell, store) = setup(vec![("umask", 1)], Store(Ok(VALUE)));
    let e = with_remote_env(&shell, &store, "t", "my-secret", PATH, "do-it")
        .await
        .unwrap_err();
    assert!(e.to_string().contains("could not write"), "{e}");
    assert_eq!(
        commands(&log),
        [
            "umask 077 && cat > '/dev/shm/x.env'",
            "rm -f '/dev/shm/x.env'"
        ]
    );
}

#[tokio::test]
async fn a_file_that_cannot_be_removed_is_an_error_not_a_silence() {
    let (_, shell, store) = setup(vec![("rm", 1)], Store(Ok(VALUE)));
    let e = with_remote_env(&shell, &store, "t", "my-secret", PATH, "do-it")
        .await
        .unwrap_err();
    assert!(e.to_string().contains("could not remove"), "{e}");
}

#[tokio::test]
async fn a_missing_secret_touches_nothing_on_the_server() {
    let (log, shell, store) = setup(vec![], Store(Err("no such secret")));
    let e = with_remote_env(&shell, &store, "t", "my-secret", PATH, "do-it")
        .await
        .unwrap_err();
    assert!(e.to_string().contains("no such secret"));
    assert!(commands(&log).is_empty());
}

#[tokio::test]
async fn an_unsafe_path_or_name_is_refused_before_anything_runs() {
    let (log, shell, store) = setup(vec![], Store(Ok(VALUE)));
    for path in ["/tmp/x", "/dev/shm/a'b"] {
        let e = with_remote_env(&shell, &store, "t", "my-secret", path, "c")
            .await
            .unwrap_err();
        assert!(matches!(e, DomainError::ValidationError(_)), "{path}");
    }
    let e = with_remote_env(&shell, &store, "t", "Bad Name", PATH, "c")
        .await
        .unwrap_err();
    assert!(matches!(e, DomainError::ValidationError(_)));
    assert!(commands(&log).is_empty());
}
