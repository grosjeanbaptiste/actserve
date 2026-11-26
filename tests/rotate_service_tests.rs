//! `RotateService`: credential rotation of the API, with every port faked.
mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::application::deploy_service::DeployService;
use actserve::application::harden_service::{HardenRun, exit_code, render};
use actserve::application::rotate_service::{RotatePorts, RotateService};
use actserve::domain::{
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    harden::ItemOutcome,
    rotate::RotateTarget,
    secret::SecretValue,
};
use actserve::ports::outbound::{
    hostname_probe::HostnameProbePort,
    random::RandomPort,
    remote_copy::RemoteCopyPort,
    remote_shell::{RemoteOutput, RemoteShellPort},
    secret_store::SecretStorePort,
    secret_writer::SecretWriterPort,
    server_inventory::ServerInventoryPort,
    state_persist::StatePersistPort,
};

#[derive(Default)]
struct Log(Mutex<Vec<String>>);
impl Log {
    fn push(&self, s: String) {
        self.0.lock().unwrap().push(s);
    }
    fn all(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
    fn count(&self, needle: &str) -> usize {
        self.all().iter().filter(|l| l.contains(needle)).count()
    }
}

struct Inventory;
#[async_trait]
impl ServerInventoryPort for Inventory {
    async fn hosts(&self) -> Result<Vec<FleetHost>, DomainError> {
        Ok(vec![FleetHost::new("a", "ta", "h1").unwrap()])
    }
}
struct Probe(&'static str);
#[async_trait]
impl HostnameProbePort for Probe {
    async fn hostname(&self, _: &str) -> Result<String, DomainError> {
        Ok(self.0.into())
    }
}
/// Answers 0 unless the command contains a marker; `stderr` is what a failure says.
struct Shell(Arc<Log>, Vec<(&'static str, i32)>, String);
#[async_trait]
impl RemoteShellPort for Shell {
    async fn run(&self, t: &str, c: &str, i: Option<&str>) -> Result<RemoteOutput, DomainError> {
        self.0.push(format!("shell {t} [{}] {c}", i.unwrap_or("-")));
        let code = self
            .1
            .iter()
            .find(|(p, _)| c.contains(p))
            .map_or(0, |(_, r)| *r);
        Ok(RemoteOutput {
            code: Some(code),
            stdout: String::new(),
            stderr: self.2.clone(),
        })
    }
}
struct Copy(Arc<Log>, Vec<PathBuf>);
#[async_trait]
impl RemoteCopyPort for Copy {
    async fn missing(&self, _: &[PathBuf]) -> Vec<PathBuf> {
        self.1.clone()
    }
    async fn copy(&self, t: &str, _: &[PathBuf], d: &str) -> Result<RemoteOutput, DomainError> {
        self.0.push(format!("copy {t} -> {d}"));
        Ok(RemoteOutput {
            code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}
struct Store(Arc<Log>, Vec<&'static str>);
#[async_trait]
impl SecretStorePort for Store {
    async fn get(&self, n: &str) -> Result<SecretValue, DomainError> {
        self.0.push(format!("get {n}"));
        if self.1.contains(&n) {
            return Err(DomainError::ExecutionFailed {
                reason: format!("{n} locked"),
                details: None,
            });
        }
        if n == "acteble-api-prod" {
            return Ok(SecretValue::new(common::full_api_env()));
        }
        Ok(SecretValue::new(format!("value-of-{n}")))
    }
}
struct Writer(Arc<Log>, Arc<Mutex<Vec<(String, String)>>>, bool);
#[async_trait]
impl SecretWriterPort for Writer {
    async fn set_values(&self, n: &str, v: &[(String, SecretValue)]) -> Result<(), DomainError> {
        let keys: Vec<_> = v.iter().map(|(k, _)| k.as_str()).collect();
        self.0.push(format!("set {n} {}", keys.join(",")));
        if self.2 {
            return Err(DomainError::ExecutionFailed {
                reason: "kv-set exited 1".into(),
                details: None,
            });
        }
        let mut seen = self.1.lock().unwrap();
        seen.extend(v.iter().map(|(k, s)| (k.clone(), s.expose().to_string())));
        Ok(())
    }
}
/// Fills with 0xab and logs the sizes asked, or fails.
struct Random(Arc<Log>, bool);
impl RandomPort for Random {
    fn bytes(&self, n: usize) -> Result<Vec<u8>, DomainError> {
        self.0.push(format!("random {n}"));
        if self.1 {
            return Err(DomainError::ExecutionFailed {
                reason: "no entropy".into(),
                details: None,
            });
        }
        Ok(vec![0xab; n])
    }
}
struct Persist(Arc<Log>, bool);
#[async_trait]
impl StatePersistPort for Persist {
    async fn persist(&self) -> Result<(), DomainError> {
        self.0.push("persist".into());
        if self.1 {
            return Err(DomainError::ExecutionFailed {
                reason: "push rejected".into(),
                details: None,
            });
        }
        Ok(())
    }
}

struct Opts {
    host: &'static str,
    shell: Vec<(&'static str, i32)>,
    stderr: &'static str,
    missing: Vec<PathBuf>,
    locked: Vec<&'static str>,
    write_fails: bool,
    random_fails: bool,
    persist_fails: bool,
    db_user: &'static str,
}
fn opts() -> Opts {
    Opts {
        host: "h1",
        shell: vec![],
        stderr: "boom",
        missing: vec![],
        locked: vec![],
        write_fails: false,
        random_fails: false,
        persist_fails: false,
        db_user: "acteble_prod",
    }
}
type Written = Arc<Mutex<Vec<(String, String)>>>;
fn service(o: Opts) -> (RotateService, Arc<Log>, Written) {
    let log = Arc::new(Log::default());
    let written: Written = Arc::default();
    let shell = || Shell(log.clone(), o.shell.clone(), o.stderr.into());
    let deploy = DeployService::new(
        Arc::new(Inventory),
        Arc::new(Probe(o.host)),
        Arc::new(shell()),
        Arc::new(Copy(log.clone(), o.missing.clone())),
        Arc::new(Store(log.clone(), o.locked.clone())),
    );
    let ports = RotatePorts {
        inventory: Arc::new(Inventory),
        probe: Arc::new(Probe(o.host)),
        shell: Arc::new(shell()),
        secrets: Arc::new(Store(log.clone(), o.locked.clone())),
        writer: Arc::new(Writer(log.clone(), written.clone(), o.write_fails)),
        random: Arc::new(Random(log.clone(), o.random_fails)),
        persist: Arc::new(Persist(log.clone(), o.persist_fails)),
    };
    (
        RotateService::new(ports, Arc::new(deploy), o.db_user),
        log,
        written,
    )
}
fn dir() -> &'static Path {
    Path::new("/src/api")
}
fn items(run: &HardenRun) -> Vec<(&'static str, &'static str, ItemOutcome)> {
    match run {
        HardenRun::Done(v) => v
            .iter()
            .map(|i| (i.step, i.item, i.outcome.clone()))
            .collect(),
        HardenRun::Refused(s) => panic!("refused {s:?}"),
    }
}
async fn rotate(o: Opts, target: RotateTarget, dry: bool) -> (HardenRun, Arc<Log>, Written) {
    let (s, log, written) = service(o);
    let run = s.rotate("a", target, dir(), dry).await.unwrap();
    (run, log, written)
}
fn pos(log: &Log, needle: &str) -> usize {
    log.all()
        .iter()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("{needle} not in {:?}", log.all()))
}

#[tokio::test]
async fn a_dry_run_names_every_step_and_changes_nothing() {
    let (run, log, written) = rotate(opts(), RotateTarget::All, true).await;
    assert_eq!(
        items(&run),
        [
            ("rotate", "database", ItemOutcome::WouldApply),
            ("rotate", "secret", ItemOutcome::WouldApply),
            ("rotate", "persist", ItemOutcome::WouldApply),
            ("deploy", "prepare", ItemOutcome::WouldApply),
            ("deploy", "copy", ItemOutcome::WouldApply),
            ("deploy", "restart", ItemOutcome::WouldApply),
        ]
    );
    assert_eq!(exit_code(&run), 0);
    assert!(log.all().is_empty(), "{:?}", log.all());
    assert!(written.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_dry_run_of_jwt_has_no_database_step() {
    let (run, _, _) = rotate(opts(), RotateTarget::Jwt, true).await;
    let names: Vec<_> = items(&run).iter().map(|(_, i, _)| *i).collect();
    assert_eq!(names, ["secret", "persist", "prepare", "copy", "restart"]);
}

#[tokio::test]
async fn a_wrong_host_is_refused_and_nothing_runs() {
    for dry in [false, true] {
        let (s, log, _) = service(Opts {
            host: "other",
            ..opts()
        });
        let run = s.rotate("a", RotateTarget::All, dir(), dry).await.unwrap();
        assert!(matches!(
            run,
            HardenRun::Refused(HostStatus::WrongHost { .. })
        ));
        assert_eq!(exit_code(&run), 3);
        assert!(log.all().is_empty());
    }
}

#[tokio::test]
async fn an_unknown_server_is_not_found() {
    let (s, _, _) = service(opts());
    let e = s
        .rotate("zz", RotateTarget::Jwt, dir(), false)
        .await
        .unwrap_err();
    assert!(matches!(e, DomainError::NotFound { .. }), "{e:?}");
}

#[tokio::test]
async fn a_missing_api_file_stops_everything_before_a_secret_is_read() {
    for dry in [false, true] {
        let (s, log, _) = service(Opts {
            missing: vec!["/src/api/server/quadlet/acteble_api.network".into()],
            ..opts()
        });
        let e = s
            .rotate("a", RotateTarget::All, dir(), dry)
            .await
            .unwrap_err();
        assert!(
            matches!(&e, DomainError::ValidationError(m) if m.contains("acteble_api.network")),
            "{e:?}"
        );
        assert!(log.all().is_empty(), "{:?}", log.all());
    }
}

#[tokio::test]
async fn a_bad_database_user_stops_a_postgres_rotation_before_any_remote_call() {
    let (s, log, _) = service(Opts {
        db_user: "a'b",
        ..opts()
    });
    let e = s
        .rotate("a", RotateTarget::Postgres, dir(), false)
        .await
        .unwrap_err();
    assert!(matches!(e, DomainError::ValidationError(_)), "{e:?}");
    assert!(log.all().is_empty());
}

#[tokio::test]
async fn the_database_user_does_not_matter_when_the_database_is_left_alone() {
    let (run, _, _) = rotate(
        Opts {
            db_user: "a'b",
            ..opts()
        },
        RotateTarget::Jwt,
        false,
    )
    .await;
    assert_eq!(exit_code(&run), 0);
}

#[tokio::test]
async fn a_locked_secret_stops_the_rotation_before_anything_changes() {
    for locked in ["acteble-api-prod", "ghcr-read-packages"] {
        let (run, log, written) = rotate(
            Opts {
                locked: vec![locked],
                ..opts()
            },
            RotateTarget::All,
            false,
        )
        .await;
        assert_eq!(exit_code(&run), 1);
        assert!(render("a", &run).contains(&format!("{locked} locked")));
        assert_eq!(log.count("random"), 0);
        assert_eq!(log.count("shell"), 0);
        assert_eq!(log.count("set "), 0);
        assert!(written.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn no_entropy_stops_the_rotation_before_anything_changes() {
    let (run, log, _) = rotate(
        Opts {
            random_fails: true,
            ..opts()
        },
        RotateTarget::All,
        false,
    )
    .await;
    assert_eq!(exit_code(&run), 1);
    assert!(render("a", &run).contains("no entropy"));
    assert_eq!(log.count("shell"), 0);
    assert_eq!(log.count("set "), 0);
}

#[tokio::test]
async fn a_postgres_rotation_changes_the_database_then_the_secret_then_persists_then_deploys() {
    let (run, log, written) = rotate(opts(), RotateTarget::Postgres, false).await;
    assert_eq!(exit_code(&run), 0, "{}", render("a", &run));
    assert_eq!(
        items(&run),
        [
            ("rotate", "database", ItemOutcome::Applied),
            ("rotate", "secret", ItemOutcome::Applied),
            ("rotate", "persist", ItemOutcome::Applied),
            ("deploy", "prepare", ItemOutcome::Applied),
            ("deploy", "copy", ItemOutcome::Applied),
            ("deploy", "restart", ItemOutcome::Applied),
        ]
    );
    let password = "ab".repeat(32);
    let alter = pos(&log, "ALTER USER");
    assert!(log.all()[alter].contains(&format!("PASSWORD '{password}'")));
    assert!(log.all()[alter].contains("psql -U 'acteble_prod' -v ON_ERROR_STOP=1"));
    assert!(alter < pos(&log, "set acteble-api-prod POSTGRES_PASSWORD"));
    assert!(pos(&log, "set acteble-api-prod") < pos(&log, "persist"));
    assert!(pos(&log, "persist") < pos(&log, "copy ta"));
    assert_eq!(
        *written.lock().unwrap(),
        [("POSTGRES_PASSWORD".to_string(), password)]
    );
    assert_eq!(log.count("random 32"), 1);
    // The deploy read the secrets again, after the change.
    assert!(log.count("get acteble-api-prod") >= 2);
}

#[tokio::test]
async fn the_password_travels_on_stdin_never_in_a_command_line() {
    let (_, log, _) = rotate(opts(), RotateTarget::Postgres, false).await;
    let password = "ab".repeat(32);
    for line in log.all().iter().filter(|l| l.starts_with("shell")) {
        let (stdin, command) = line.split_once("] ").unwrap();
        assert!(!command.contains(&password), "{line}");
        assert!(stdin.contains(&password) || !line.contains("ALTER USER"));
    }
}

#[tokio::test]
async fn all_rotates_four_keys_of_the_documented_sizes_in_one_write() {
    let (run, log, written) = rotate(opts(), RotateTarget::All, false).await;
    assert_eq!(exit_code(&run), 0, "{}", render("a", &run));
    assert_eq!(log.count("set acteble-api-prod"), 1);
    let w = written.lock().unwrap().clone();
    let keys: Vec<_> = w.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        keys,
        [
            "APP_AUTH__JWT_SECRET",
            "POSTGRES_PASSWORD",
            "MINIO_ROOT_USER",
            "MINIO_ROOT_PASSWORD"
        ]
    );
    assert_eq!(w[0].1.len(), 88);
    assert_eq!(w[2].1.len(), 24);
    let sizes: Vec<_> = log
        .all()
        .into_iter()
        .filter(|l| l.starts_with("random"))
        .collect();
    assert_eq!(sizes, ["random 64", "random 32", "random 12", "random 32"]);
}

#[tokio::test]
async fn jwt_and_minio_leave_the_database_alone() {
    for target in [RotateTarget::Jwt, RotateTarget::Minio] {
        let (run, log, _) = rotate(opts(), target, false).await;
        assert_eq!(exit_code(&run), 0, "{}", render("a", &run));
        assert_eq!(log.count("ALTER USER"), 0);
        assert!(items(&run).iter().all(|(_, i, _)| *i != "database"));
    }
}

#[tokio::test]
async fn a_failed_database_change_leaves_the_secret_untouched_and_deploys_nothing() {
    let (run, log, written) = rotate(
        Opts {
            shell: vec![("psql", 1)],
            stderr: "psql: role missing",
            ..opts()
        },
        RotateTarget::Postgres,
        false,
    )
    .await;
    assert_eq!(exit_code(&run), 1);
    let text = render("a", &run);
    assert!(text.contains("psql: role missing"), "{text}");
    assert!(text.contains("secret untouched"), "{text}");
    assert_eq!(log.count("set "), 0);
    assert_eq!(log.count("persist"), 0);
    assert_eq!(log.count("copy ta"), 0);
    assert!(written.lock().unwrap().is_empty());
    assert_eq!(items(&run).len(), 1);
}

#[tokio::test]
async fn a_failure_message_never_shows_the_new_password() {
    let leak = format!("LINE 1: ALTER USER x WITH PASSWORD '{}'", "ab".repeat(32));
    let (run, _, _) = rotate(
        Opts {
            shell: vec![("psql", 1)],
            stderr: Box::leak(leak.into_boxed_str()),
            ..opts()
        },
        RotateTarget::Postgres,
        false,
    )
    .await;
    let text = render("a", &run);
    assert!(!text.contains(&"ab".repeat(32)), "{text}");
    assert!(text.contains("PASSWORD '***'"), "{text}");
}

#[tokio::test]
async fn a_failed_secret_write_after_a_database_change_says_the_database_may_be_rotated() {
    let (run, log, _) = rotate(
        Opts {
            write_fails: true,
            ..opts()
        },
        RotateTarget::Postgres,
        false,
    )
    .await;
    assert_eq!(exit_code(&run), 1);
    let text = render("a", &run);
    assert!(text.contains("kv-set exited 1"), "{text}");
    assert!(
        text.contains("database password may already be rotated"),
        "{text}"
    );
    assert_eq!(log.count("persist"), 0);
    assert_eq!(log.count("copy ta"), 0);
}

#[tokio::test]
async fn a_failed_secret_write_without_a_database_change_does_not_blame_the_database() {
    let (run, _, _) = rotate(
        Opts {
            write_fails: true,
            ..opts()
        },
        RotateTarget::Jwt,
        false,
    )
    .await;
    assert_eq!(exit_code(&run), 1);
    assert!(!render("a", &run).contains("database"));
}

#[tokio::test]
async fn a_failed_persist_is_reported_but_the_deploy_still_runs() {
    let (run, log, _) = rotate(
        Opts {
            persist_fails: true,
            ..opts()
        },
        RotateTarget::Jwt,
        false,
    )
    .await;
    assert_eq!(exit_code(&run), 1);
    let text = render("a", &run);
    assert!(text.contains("push rejected"), "{text}");
    assert!(text.contains("machine autocommit"), "{text}");
    assert_eq!(log.count("copy ta"), 1);
    let last = items(&run).pop().unwrap();
    assert_eq!(last, ("deploy", "restart", ItemOutcome::Applied));
}

#[tokio::test]
async fn a_failed_deploy_is_reported_after_the_rotation_was_made() {
    let (run, log, _) = rotate(
        Opts {
            shell: vec![("podman login", 1)],
            ..opts()
        },
        RotateTarget::Jwt,
        false,
    )
    .await;
    assert_eq!(exit_code(&run), 1);
    assert_eq!(log.count("set acteble-api-prod"), 1);
    let names: Vec<_> = items(&run).iter().map(|(_, i, _)| *i).collect();
    assert_eq!(names, ["secret", "persist", "prepare", "copy", "restart"]);
    assert!(matches!(items(&run)[4].2, ItemOutcome::Failed(_)));
}

#[tokio::test]
async fn no_new_value_ever_appears_in_the_report() {
    let (run, _, written) = rotate(opts(), RotateTarget::All, false).await;
    let text = render("a", &run);
    for (_, value) in written.lock().unwrap().iter() {
        assert!(!text.contains(value.as_str()), "{text}");
    }
}
