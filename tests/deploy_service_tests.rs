//! `DeployService`: deploy and rollback of the API, with every port faked.
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::application::deploy_service::DeployService;
use actserve::application::harden_service::{HardenRun, exit_code, render};
use actserve::domain::{
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    harden::ItemOutcome,
    secret::SecretValue,
};
use actserve::ports::outbound::{
    hostname_probe::HostnameProbePort,
    remote_copy::RemoteCopyPort,
    remote_shell::{RemoteOutput, RemoteShellPort},
    secret_store::SecretStorePort,
    server_inventory::ServerInventoryPort,
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
struct Shell(Arc<Log>, Vec<(&'static str, i32)>);
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
            stderr: "boom".into(),
        })
    }
}
struct Copy(Arc<Log>, Vec<PathBuf>, i32);
#[async_trait]
impl RemoteCopyPort for Copy {
    async fn missing(&self, _: &[PathBuf]) -> Vec<PathBuf> {
        self.1.clone()
    }
    async fn copy(&self, t: &str, f: &[PathBuf], d: &str) -> Result<RemoteOutput, DomainError> {
        let names: Vec<_> = f
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy())
            .collect();
        self.0.push(format!("copy {t} {} -> {d}", names.join(",")));
        Ok(RemoteOutput {
            code: Some(self.2),
            stdout: String::new(),
            stderr: "scp boom".into(),
        })
    }
}
struct Store(Arc<Log>, Vec<&'static str>);
#[async_trait]
impl SecretStorePort for Store {
    async fn get(&self, n: &str) -> Result<SecretValue, DomainError> {
        self.0.push(format!("secret {n}"));
        if self.1.contains(&n) {
            return Err(DomainError::ExecutionFailed {
                reason: format!("{n} locked"),
                details: None,
            });
        }
        Ok(SecretValue::new(format!("value-of-{n}")))
    }
}

struct Opts {
    host: &'static str,
    shell: Vec<(&'static str, i32)>,
    missing: Vec<PathBuf>,
    copy_rc: i32,
    locked: Vec<&'static str>,
}
fn opts() -> Opts {
    Opts {
        host: "h1",
        shell: vec![],
        missing: vec![],
        copy_rc: 0,
        locked: vec![],
    }
}
fn service(o: Opts) -> (DeployService, Arc<Log>) {
    let log = Arc::new(Log::default());
    let s = DeployService::new(
        Arc::new(Inventory),
        Arc::new(Probe(o.host)),
        Arc::new(Shell(log.clone(), o.shell)),
        Arc::new(Copy(log.clone(), o.missing, o.copy_rc)),
        Arc::new(Store(log.clone(), o.locked)),
    );
    (s, log)
}
fn dir() -> &'static Path {
    Path::new("/src/api")
}
fn items(run: &HardenRun) -> Vec<(&'static str, ItemOutcome)> {
    match run {
        HardenRun::Done(v) => v.iter().map(|i| (i.item, i.outcome.clone())).collect(),
        HardenRun::Refused(s) => panic!("refused {s:?}"),
    }
}

#[tokio::test]
async fn a_deploy_reads_both_secrets_copies_then_restarts_and_cleans_up() {
    let (s, log) = service(opts());
    let run = s.deploy("a", dir(), false).await.unwrap();
    assert_eq!(
        items(&run),
        [
            ("copy", ItemOutcome::Applied),
            ("restart", ItemOutcome::Applied)
        ]
    );
    assert_eq!(exit_code(&run), 0);
    let l = log.all();
    assert_eq!(l[0], "secret ghcr-read-packages");
    assert_eq!(l[1], "secret acteble-api-prod");
    assert_eq!(
        l[2],
        "copy ta docker-compose.yml,docker-compose.prod.yml,docker-compose.ghcr.yml,Caddyfile -> /opt/acteble/api"
    );
    assert_eq!(
        l[3],
        "shell ta [value-of-acteble-api-prod] umask 077 && cat > '/dev/shm/acteble-api.env'"
    );
    assert!(
        l[4].starts_with("shell ta [value-of-ghcr-read-packages] docker login ghcr.io"),
        "{}",
        l[4]
    );
    assert!(
        l[4].contains("docker image tag") && l[4].contains("up -d --no-build --remove-orphans")
    );
    assert_eq!(l[5], "shell ta [-] rm -f '/dev/shm/acteble-api.env'");
    assert_eq!(l.len(), 6);
}

#[tokio::test]
async fn a_dry_run_touches_nothing_and_reads_no_secret() {
    let (s, log) = service(opts());
    let run = s.deploy("a", dir(), true).await.unwrap();
    assert_eq!(
        items(&run),
        [
            ("copy", ItemOutcome::WouldApply),
            ("restart", ItemOutcome::WouldApply)
        ]
    );
    assert!(log.all().is_empty());
    assert_eq!(
        render("a", &run),
        "deploy copy      would apply\ndeploy restart   would apply\n"
    );
}

#[tokio::test]
async fn a_missing_local_file_is_a_validation_error_in_both_modes_and_runs_nothing() {
    for dry in [false, true] {
        let (s, log) = service(Opts {
            missing: vec!["/src/api/Caddyfile".into()],
            ..opts()
        });
        let e = s.deploy("a", dir(), dry).await.unwrap_err();
        assert!(
            matches!(&e, DomainError::ValidationError(m) if m.contains("Caddyfile")),
            "{e:?}"
        );
        assert!(log.all().is_empty());
    }
}

#[tokio::test]
async fn a_failed_copy_stops_before_any_secret_reaches_the_server() {
    let (s, log) = service(Opts {
        copy_rc: 1,
        ..opts()
    });
    let run = s.deploy("a", dir(), false).await.unwrap();
    let it = items(&run);
    assert!(
        matches!(&it[0], ("copy", ItemOutcome::Failed(m)) if m.contains("scp boom")),
        "{it:?}"
    );
    assert_eq!(it.len(), 1);
    assert_eq!(exit_code(&run), 1);
    assert!(!log.all().iter().any(|l| l.starts_with("shell")));
}

#[tokio::test]
async fn a_failed_restart_is_reported_and_the_env_file_is_still_removed() {
    let (s, log) = service(Opts {
        shell: vec![("docker login", 5)],
        ..opts()
    });
    let run = s.deploy("a", dir(), false).await.unwrap();
    let it = items(&run);
    assert!(
        matches!(&it[1], ("restart", ItemOutcome::Failed(m)) if m.contains("exit 5")),
        "{it:?}"
    );
    assert_eq!(exit_code(&run), 1);
    assert!(
        log.all()
            .last()
            .unwrap()
            .contains("rm -f '/dev/shm/acteble-api.env'")
    );
}

#[tokio::test]
async fn a_locked_secret_fails_before_anything_is_copied() {
    for locked in ["ghcr-read-packages", "acteble-api-prod"] {
        let (s, log) = service(Opts {
            locked: vec![locked],
            ..opts()
        });
        let run = s.deploy("a", dir(), false).await.unwrap();
        let it = items(&run);
        assert!(
            matches!(&it[0], ("secrets", ItemOutcome::Failed(m)) if m.contains("locked")),
            "{it:?}"
        );
        assert_eq!(exit_code(&run), 1);
        assert!(
            !log.all()
                .iter()
                .any(|l| l.starts_with("copy") || l.starts_with("shell"))
        );
    }
}

#[tokio::test]
async fn the_wrong_host_is_refused_and_nothing_runs() {
    let (s, log) = service(Opts {
        host: "other",
        ..opts()
    });
    for run in [
        s.deploy("a", dir(), false).await.unwrap(),
        s.rollback("a", "previous", false).await.unwrap(),
    ] {
        assert!(matches!(
            run,
            HardenRun::Refused(HostStatus::WrongHost { .. })
        ));
        assert_eq!(exit_code(&run), 3);
    }
    assert!(log.all().is_empty());
}

#[tokio::test]
async fn an_unknown_server_is_not_found() {
    let (s, _) = service(opts());
    assert!(matches!(
        s.deploy("zz", dir(), false).await.unwrap_err(),
        DomainError::NotFound { .. }
    ));
}

#[tokio::test]
async fn a_rollback_restarts_on_the_tag_with_the_same_secret_handling() {
    let (s, log) = service(opts());
    let run = s.rollback("a", "sha-abc", false).await.unwrap();
    assert_eq!(items(&run), [("restart", ItemOutcome::Applied)]);
    let l = log.all();
    assert_eq!(
        &l[..2],
        ["secret ghcr-read-packages", "secret acteble-api-prod"]
    );
    assert!(
        l[3].contains("API_IMAGE='ghcr.io/acteble/acteble_api:sha-abc'"),
        "{}",
        l[3]
    );
    assert_eq!(
        l.last().unwrap(),
        "shell ta [-] rm -f '/dev/shm/acteble-api.env'"
    );
    assert!(!l.iter().any(|x| x.starts_with("copy")));
    assert_eq!(render("a", &run), "rollback restart   applied\n");
}

#[tokio::test]
async fn a_rollback_dry_run_and_a_bad_tag_touch_nothing() {
    let (s, log) = service(opts());
    let run = s.rollback("a", "previous", true).await.unwrap();
    assert_eq!(items(&run), [("restart", ItemOutcome::WouldApply)]);
    let e = s.rollback("a", "x y", false).await.unwrap_err();
    assert!(matches!(e, DomainError::ValidationError(_)));
    assert!(log.all().is_empty());
}
