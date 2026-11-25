//! `GeoBuildService`: the geo build on the CDN host, every port faked.
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::application::geo_build_service::GeoBuildService;
use actserve::application::harden_service::{HardenRun, exit_code, render};
use actserve::domain::{
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    geo_build::GeoStage,
    harden::ItemOutcome,
    secret::SecretValue,
};
use actserve::ports::outbound::{
    hostname_probe::HostnameProbePort,
    remote_shell::{RemoteOutput, RemoteShellPort},
    remote_terminal::RemoteTerminalPort,
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
struct Terminal(Arc<Log>, Result<Option<i32>, &'static str>);
#[async_trait]
impl RemoteTerminalPort for Terminal {
    async fn attach(&self, t: &str, c: &str) -> Result<Option<i32>, DomainError> {
        self.0.push(format!("attach {t} {c}"));
        self.1.map_err(|m| DomainError::ExecutionFailed {
            reason: m.into(),
            details: None,
        })
    }
}
/// `env` is what the cdn secret holds; the ghcr secret is `ghcr-value`.
struct Store(Arc<Log>, &'static str, Vec<&'static str>);
#[async_trait]
impl SecretStorePort for Store {
    async fn get(&self, n: &str) -> Result<SecretValue, DomainError> {
        self.0.push(format!("secret {n}"));
        if self.2.contains(&n) {
            return Err(DomainError::ExecutionFailed {
                reason: format!("{n} locked"),
                details: None,
            });
        }
        Ok(SecretValue::new(if n == "acteble-cdn-prod" {
            self.1.to_string()
        } else {
            "ghcr-value".to_string()
        }))
    }
}

const GOOD_ENV: &str = "MINIO_ROOT_USER=u\nCDN_MANIFEST_SIGNING_KEY=sign-me\n";
struct Opts {
    host: &'static str,
    shell: Vec<(&'static str, i32)>,
    term: Result<Option<i32>, &'static str>,
    env: &'static str,
    locked: Vec<&'static str>,
}
fn opts() -> Opts {
    Opts {
        host: "h1",
        shell: vec![],
        term: Ok(Some(0)),
        env: GOOD_ENV,
        locked: vec![],
    }
}
fn service(o: Opts) -> (GeoBuildService, Arc<Log>) {
    let log = Arc::new(Log::default());
    let s = GeoBuildService::new(
        Arc::new(Inventory),
        Arc::new(Probe(o.host)),
        Arc::new(Shell(log.clone(), o.shell)),
        Arc::new(Terminal(log.clone(), o.term)),
        Arc::new(Store(log.clone(), o.env, o.locked)),
    );
    (s, log)
}
fn items(run: &HardenRun) -> Vec<(&'static str, ItemOutcome)> {
    match run {
        HardenRun::Done(v) => v.iter().map(|i| (i.item, i.outcome.clone())).collect(),
        HardenRun::Refused(s) => panic!("refused {s:?}"),
    }
}
fn failed_with(run: &HardenRun, item: &str) -> String {
    match items(run).into_iter().find(|(n, _)| *n == item) {
        Some((_, ItemOutcome::Failed(m))) => m,
        other => panic!("{item}: {other:?}"),
    }
}

#[tokio::test]
async fn a_build_starts_detached_then_follows_and_reports_both() {
    let (s, log) = service(opts());
    let run = s.build("a", GeoStage::All, false).await.unwrap();
    assert_eq!(
        items(&run),
        vec![
            ("start", ItemOutcome::Applied),
            ("follow", ItemOutcome::Applied)
        ]
    );
    assert_eq!(exit_code(&run), 0);
    let all = log.all();
    assert!(
        all.iter()
            .any(|l| l.starts_with("attach ta docker logs -f acteble-geo-build"))
    );
    assert!(
        all.iter()
            .any(|l| l.contains("geo_builder --stage all") && l.contains("docker login ghcr.io"))
    );
}

#[tokio::test]
async fn the_env_goes_to_the_tmpfs_by_stdin_and_is_removed_after() {
    let (s, log) = service(opts());
    s.build("a", GeoStage::Tree, false).await.unwrap();
    let all = log.all();
    let write = all
        .iter()
        .position(|l| l.contains("cat > '/dev/shm/acteble-cdn.env'"))
        .unwrap();
    let start = all.iter().position(|l| l.contains("--stage tree")).unwrap();
    let remove = all
        .iter()
        .position(|l| l.contains("rm -f '/dev/shm/acteble-cdn.env'"))
        .unwrap();
    assert!(write < start && start < remove, "{all:?}");
    assert!(
        all[write].contains("[MINIO_ROOT_USER=u"),
        "env travels on stdin"
    );
}

#[tokio::test]
async fn the_ghcr_token_travels_on_stdin_never_in_an_argument() {
    let (s, log) = service(opts());
    s.build("a", GeoStage::All, false).await.unwrap();
    let start = log
        .all()
        .into_iter()
        .find(|l| l.contains("--stage all"))
        .unwrap();
    assert!(start.contains("[ghcr-value]"), "{start}");
    assert!(!start.replace("[ghcr-value]", "").contains("ghcr-value"));
}

#[tokio::test]
async fn the_follow_runs_after_the_env_file_is_gone() {
    let (s, log) = service(opts());
    s.build("a", GeoStage::All, false).await.unwrap();
    let all = log.all();
    let remove = all
        .iter()
        .position(|l| l.contains("rm -f '/dev/shm/acteble-cdn.env'"))
        .unwrap();
    let attach = all.iter().position(|l| l.starts_with("attach")).unwrap();
    assert!(remove < attach, "{all:?}");
}

#[tokio::test]
async fn another_host_is_refused_and_nothing_runs() {
    let (s, log) = service(Opts {
        host: "other",
        ..opts()
    });
    let run = s.build("a", GeoStage::All, false).await.unwrap();
    assert!(
        matches!(run, HardenRun::Refused(HostStatus::WrongHost { .. })),
        "{run:?}"
    );
    assert!(log.all().is_empty());
}

#[tokio::test]
async fn an_unknown_name_is_not_found() {
    let (s, _) = service(opts());
    let e = s.build("zz", GeoStage::All, false).await.unwrap_err();
    assert!(matches!(e, DomainError::NotFound { .. }), "{e:?}");
}

#[tokio::test]
async fn a_dry_run_lists_both_steps_and_reads_and_runs_nothing() {
    let (s, log) = service(opts());
    let run = s.build("a", GeoStage::All, true).await.unwrap();
    assert_eq!(
        items(&run),
        vec![
            ("start", ItemOutcome::WouldApply),
            ("follow", ItemOutcome::WouldApply)
        ]
    );
    assert!(log.all().is_empty(), "{:?}", log.all());
}

#[tokio::test]
async fn a_locked_secret_stops_before_the_server_is_touched() {
    for locked in ["ghcr-read-packages", "acteble-cdn-prod"] {
        let (s, log) = service(Opts {
            locked: vec![locked],
            ..opts()
        });
        let run = s.build("a", GeoStage::All, false).await.unwrap();
        assert!(failed_with(&run, "secrets").contains("locked"));
        assert_eq!(exit_code(&run), 1);
        assert!(
            log.all().iter().all(|l| l.starts_with("secret ")),
            "{:?}",
            log.all()
        );
    }
}

#[tokio::test]
async fn a_missing_signing_key_refuses_before_the_server_is_touched() {
    for env in ["MINIO_ROOT_USER=u\n", "CDN_MANIFEST_SIGNING_KEY=\n", ""] {
        let (s, log) = service(Opts { env, ..opts() });
        let run = s.build("a", GeoStage::All, false).await.unwrap();
        let m = failed_with(&run, "secrets");
        assert!(
            m.contains("CDN_MANIFEST_SIGNING_KEY") && m.contains("acteble-cdn-prod"),
            "{m}"
        );
        assert!(log.all().iter().all(|l| l.starts_with("secret ")));
    }
}

#[tokio::test]
async fn no_failure_message_carries_a_secret_value() {
    let (s, _) = service(Opts {
        shell: vec![("--stage", 1)],
        ..opts()
    });
    let out = render("a", &s.build("a", GeoStage::All, false).await.unwrap());
    assert!(
        !out.contains("sign-me") && !out.contains("ghcr-value"),
        "{out}"
    );
}

#[tokio::test]
async fn a_failed_start_stops_without_following_and_still_removes_the_env() {
    let (s, log) = service(Opts {
        shell: vec![("--stage", 1)],
        ..opts()
    });
    let run = s.build("a", GeoStage::All, false).await.unwrap();
    assert_eq!(items(&run).len(), 1);
    assert!(failed_with(&run, "start").contains("exit 1"));
    let all = log.all();
    assert!(all.iter().all(|l| !l.starts_with("attach")));
    assert!(
        all.iter()
            .any(|l| l.contains("rm -f '/dev/shm/acteble-cdn.env'"))
    );
}

#[tokio::test]
async fn a_failed_build_is_reported_with_its_exit_code() {
    let (s, _) = service(Opts {
        term: Ok(Some(3)),
        ..opts()
    });
    let run = s.build("a", GeoStage::All, false).await.unwrap();
    assert_eq!(items(&run)[0], ("start", ItemOutcome::Applied));
    assert!(failed_with(&run, "follow").contains("exit 3"));
    assert_eq!(exit_code(&run), 1);
}

#[tokio::test]
async fn a_follow_killed_by_a_signal_is_a_failure_not_a_success() {
    let (s, _) = service(Opts {
        term: Ok(None),
        ..opts()
    });
    let run = s.build("a", GeoStage::All, false).await.unwrap();
    assert!(failed_with(&run, "follow").contains("signal"));
}

#[tokio::test]
async fn a_follow_that_cannot_start_is_a_failure_naming_why() {
    let (s, _) = service(Opts {
        term: Err("no tty"),
        ..opts()
    });
    let run = s.build("a", GeoStage::All, false).await.unwrap();
    assert!(failed_with(&run, "follow").contains("no tty"));
}

#[tokio::test]
async fn the_stage_reaches_the_server() {
    for stage in [
        GeoStage::Catalog,
        GeoStage::Tree,
        GeoStage::Publish,
        GeoStage::All,
    ] {
        let (s, log) = service(opts());
        s.build("a", stage, false).await.unwrap();
        let want = format!("--stage {}", stage.as_str());
        assert!(log.all().iter().any(|l| l.contains(&want)), "{want}");
    }
}
