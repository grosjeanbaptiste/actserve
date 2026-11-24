//! `CaddyDeployService`: the Caddy-only deploy, every port faked.
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::application::caddy_deploy_service::CaddyDeployService;
use actserve::application::harden_service::{HardenRun, exit_code, render};
use actserve::domain::{
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    harden::ItemOutcome,
    secret::SecretValue,
};
use actserve::ports::outbound::{
    caddyfile_source::CaddyfileSourcePort,
    hostname_probe::HostnameProbePort,
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
struct Source(Arc<Log>, bool);
#[async_trait]
impl CaddyfileSourcePort for Source {
    async fn origin_main(&self, repo: &Path) -> Result<String, DomainError> {
        self.0.push(format!("source {}", repo.display()));
        if self.1 {
            return Err(DomainError::ExecutionFailed {
                reason: "git fetch origin main failed".into(),
                details: Some("no network".into()),
            });
        }
        Ok("site {}\n".into())
    }
}
struct Store(Arc<Log>, bool);
#[async_trait]
impl SecretStorePort for Store {
    async fn get(&self, n: &str) -> Result<SecretValue, DomainError> {
        self.0.push(format!("secret {n}"));
        if self.1 {
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
    source_fails: bool,
    locked: bool,
}
fn opts() -> Opts {
    Opts {
        host: "h1",
        shell: vec![],
        source_fails: false,
        locked: false,
    }
}
fn service(o: Opts) -> (CaddyDeployService, Arc<Log>) {
    let log = Arc::new(Log::default());
    let s = CaddyDeployService::new(
        Arc::new(Inventory),
        Arc::new(Probe(o.host)),
        Arc::new(Shell(log.clone(), o.shell)),
        Arc::new(Source(log.clone(), o.source_fails)),
        Arc::new(Store(log.clone(), o.locked)),
    );
    (s, log)
}
fn items(run: &HardenRun) -> Vec<(&'static str, ItemOutcome)> {
    match run {
        HardenRun::Done(v) => v.iter().map(|i| (i.item, i.outcome.clone())).collect(),
        HardenRun::Refused(s) => panic!("refused {s:?}"),
    }
}
fn repo() -> &'static Path {
    Path::new("/src/api")
}

#[tokio::test]
async fn a_deploy_reads_main_and_the_secret_then_backs_up_writes_and_reloads() {
    let (s, log) = service(opts());
    let run = s.deploy_caddy("a", repo(), false).await.unwrap();
    assert_eq!(
        items(&run),
        [
            ("source", ItemOutcome::Applied),
            ("backup", ItemOutcome::Applied),
            ("copy", ItemOutcome::Applied),
            ("reload", ItemOutcome::Applied)
        ]
    );
    assert_eq!(exit_code(&run), 0);
    let l = log.all();
    assert_eq!(l[0], "source /src/api");
    assert_eq!(l[1], "secret acteble-api-prod");
    assert_eq!(
        l[2],
        "shell ta [-] cp '/opt/acteble/api/Caddyfile' '/opt/acteble/api/Caddyfile.bak'"
    );
    assert_eq!(
        l[3],
        "shell ta [site {}\n] cat > '/opt/acteble/api/Caddyfile.new' && mv -f '/opt/acteble/api/Caddyfile.new' '/opt/acteble/api/Caddyfile'"
    );
    assert_eq!(
        l[4],
        "shell ta [value-of-acteble-api-prod] umask 077 && cat > '/dev/shm/acteble-api.env'"
    );
    assert!(l[5].contains("caddy reload"), "{}", l[5]);
    assert_eq!(l[6], "shell ta [-] rm -f '/dev/shm/acteble-api.env'");
    assert_eq!(l.len(), 7);
}

#[tokio::test]
async fn a_dry_run_touches_nothing_fetches_nothing_and_reads_no_secret() {
    let (s, log) = service(opts());
    let run = s.deploy_caddy("a", repo(), true).await.unwrap();
    assert_eq!(
        render("a", &run),
        "deploy-caddy source    would apply\ndeploy-caddy backup    would apply\ndeploy-caddy copy      would apply\ndeploy-caddy reload    would apply\n"
    );
    assert!(log.all().is_empty());
}

#[tokio::test]
async fn another_host_is_refused_before_anything_runs() {
    let (s, log) = service(Opts {
        host: "other",
        ..opts()
    });
    let run = s.deploy_caddy("a", repo(), false).await.unwrap();
    assert!(matches!(
        run,
        HardenRun::Refused(HostStatus::WrongHost { .. })
    ));
    assert!(log.all().is_empty());
}

#[tokio::test]
async fn a_source_failure_stops_before_the_secret_and_the_server() {
    let (s, log) = service(Opts {
        source_fails: true,
        ..opts()
    });
    let run = s.deploy_caddy("a", repo(), false).await.unwrap();
    let got = items(&run);
    assert_eq!(got.len(), 1);
    assert!(
        matches!(&got[0], ("source", ItemOutcome::Failed(m)) if m.contains("no network")),
        "{got:?}"
    );
    assert_eq!(exit_code(&run), 1);
    assert_eq!(log.all(), ["source /src/api"]);
}

#[tokio::test]
async fn a_locked_secret_stops_before_the_server_is_touched() {
    let (s, log) = service(Opts {
        locked: true,
        ..opts()
    });
    let run = s.deploy_caddy("a", repo(), false).await.unwrap();
    let got = items(&run);
    assert!(
        matches!(&got[1], ("secrets", ItemOutcome::Failed(m)) if m.contains("locked")),
        "{got:?}"
    );
    assert_eq!(got.len(), 2);
    assert_eq!(log.all(), ["source /src/api", "secret acteble-api-prod"]);
}

#[tokio::test]
async fn a_failing_backup_stops_before_the_write() {
    let (s, log) = service(Opts {
        shell: vec![("Caddyfile.bak'", 1)],
        ..opts()
    });
    let run = s.deploy_caddy("a", repo(), false).await.unwrap();
    let got = items(&run);
    assert!(
        matches!(&got[1], ("backup", ItemOutcome::Failed(_))),
        "{got:?}"
    );
    assert_eq!(got.len(), 2);
    assert!(!log.all().iter().any(|l| l.contains("Caddyfile.new")));
}

#[tokio::test]
async fn a_failing_write_stops_before_the_reload() {
    let (s, log) = service(Opts {
        shell: vec![("Caddyfile.new", 1)],
        ..opts()
    });
    let run = s.deploy_caddy("a", repo(), false).await.unwrap();
    let got = items(&run);
    assert!(
        matches!(&got[2], ("copy", ItemOutcome::Failed(_))),
        "{got:?}"
    );
    assert_eq!(got.len(), 3);
    assert!(!log.all().iter().any(|l| l.contains("caddy reload")));
}

#[tokio::test]
async fn a_refused_reload_fails_and_the_env_file_is_still_removed() {
    let (s, log) = service(Opts {
        shell: vec![("caddy reload", 1)],
        ..opts()
    });
    let run = s.deploy_caddy("a", repo(), false).await.unwrap();
    let got = items(&run);
    assert!(
        matches!(&got[3], ("reload", ItemOutcome::Failed(m)) if m.contains("boom")),
        "{got:?}"
    );
    assert_eq!(exit_code(&run), 1);
    assert_eq!(
        log.all().last().unwrap(),
        "shell ta [-] rm -f '/dev/shm/acteble-api.env'"
    );
}
