//! `EdgeDeployService`: the edge deploy, every port faked.
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::application::edge_deploy_service::EdgeDeployService;
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
        let names: Vec<_> = f.iter().map(|p| p.display().to_string()).collect();
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
fn service(o: Opts) -> (EdgeDeployService, Arc<Log>) {
    let log = Arc::new(Log::default());
    let s = EdgeDeployService::new(
        Arc::new(Inventory),
        Arc::new(Probe(o.host)),
        Arc::new(Shell(log.clone(), o.shell)),
        Arc::new(Copy(log.clone(), o.missing, o.copy_rc)),
        Arc::new(Store(log.clone(), o.locked)),
    );
    (s, log)
}
fn dir() -> &'static Path {
    Path::new("/src/edge")
}
fn items(run: &HardenRun) -> Vec<(&'static str, ItemOutcome)> {
    match run {
        HardenRun::Done(v) => v.iter().map(|i| (i.item, i.outcome.clone())).collect(),
        HardenRun::Refused(s) => panic!("refused {s:?}"),
    }
}
fn names(run: &HardenRun) -> Vec<&'static str> {
    items(run).iter().map(|(n, _)| *n).collect()
}

#[tokio::test]
async fn a_deploy_runs_every_step_in_order_and_cleans_the_env_file() {
    let (s, log) = service(opts());
    let run = s.deploy("a", dir(), false).await.unwrap();
    assert_eq!(names(&run), ["prepare", "copy", "units", "binary", "start"]);
    assert!(items(&run).iter().all(|(_, o)| *o == ItemOutcome::Applied));
    assert_eq!(exit_code(&run), 0);
    let l = log.all();
    assert_eq!(l[0], "secret ghcr-read-packages");
    assert_eq!(l[1], "secret acteble-edge-prod");
    assert_eq!(
        l[2],
        "shell ta [-] mkdir -p '/opt/acteble/edge' \"$HOME/.config/containers/systemd\" \"$HOME/.config/systemd/user\""
    );
    assert_eq!(
        l[3],
        "copy ta /src/edge/Caddyfile,/src/edge/Caddyfile.socket,/src/edge/sites,/src/edge/quadlet -> /opt/acteble/edge"
    );
    assert_eq!(
        l[4],
        "shell ta [-] cp '/opt/acteble/edge/quadlet'/*.container '/opt/acteble/edge/quadlet'/*.network '/opt/acteble/edge/quadlet'/*.volume \"$HOME/.config/containers/systemd/\" && cp '/opt/acteble/edge/quadlet'/*.socket \"$HOME/.config/systemd/user/\""
    );
    assert!(
        l[5].starts_with("shell ta [value-of-ghcr-read-packages] podman login ghcr.io"),
        "{}",
        l[5]
    );
    assert!(
        l[5].contains("podman cp $id:/edge /opt/acteble/edge/edge"),
        "{}",
        l[5]
    );
    assert!(l[5].contains("podman logout ghcr.io") && !l[5].contains("docker"));
    assert_eq!(
        l[6],
        "shell ta [value-of-acteble-edge-prod] umask 077 && cat > '/dev/shm/acteble-edge.env'"
    );
    assert_eq!(
        l[7],
        "shell ta [-] install -m 600 /dev/shm/acteble-edge.env '/opt/acteble/edge/.env' && cd '/opt/acteble/edge' && ./edge up --runtime podman --env-file /opt/acteble/edge/.env"
    );
    assert_eq!(l[8], "shell ta [-] rm -f '/dev/shm/acteble-edge.env'");
    assert_eq!(l.len(), 9);
}

#[tokio::test]
async fn a_dry_run_touches_nothing_and_reads_no_secret() {
    let (s, log) = service(opts());
    let run = s.deploy("a", dir(), true).await.unwrap();
    assert_eq!(
        render("a", &run),
        "edge-deploy prepare   would apply\nedge-deploy copy      would apply\nedge-deploy units     would apply\nedge-deploy binary    would apply\nedge-deploy start     would apply\n"
    );
    assert!(log.all().is_empty());
}

#[tokio::test]
async fn a_missing_edge_file_is_a_validation_error_in_both_modes_and_runs_nothing() {
    for dry in [false, true] {
        let (s, log) = service(Opts {
            missing: vec!["/src/edge/sites".into()],
            ..opts()
        });
        let e = s.deploy("a", dir(), dry).await.unwrap_err();
        assert!(
            matches!(&e, DomainError::ValidationError(m) if m.contains("sites") && m.contains("ACTEBLE_EDGE_DIR")),
            "{e:?}"
        );
        assert!(log.all().is_empty());
    }
}

#[tokio::test]
async fn another_host_is_refused_before_anything_runs() {
    let (s, log) = service(Opts {
        host: "other",
        ..opts()
    });
    let run = s.deploy("a", dir(), false).await.unwrap();
    assert!(matches!(
        run,
        HardenRun::Refused(HostStatus::WrongHost { .. })
    ));
    assert!(log.all().is_empty());
}

#[tokio::test]
async fn a_locked_secret_stops_before_the_server_is_touched() {
    for locked in ["ghcr-read-packages", "acteble-edge-prod"] {
        let (s, log) = service(Opts {
            locked: vec![locked],
            ..opts()
        });
        let run = s.deploy("a", dir(), false).await.unwrap();
        let got = items(&run);
        assert!(
            matches!(&got[0], ("secrets", ItemOutcome::Failed(m)) if m.contains("locked")),
            "{got:?}"
        );
        assert_eq!(got.len(), 1);
        assert!(
            !log.all()
                .iter()
                .any(|l| l.starts_with("shell") || l.starts_with("copy"))
        );
    }
}

#[tokio::test]
async fn each_failing_step_stops_the_ones_after_it() {
    let cases: [(Opts, &str, usize); 4] = [
        (
            Opts {
                shell: vec![("mkdir", 1)],
                ..opts()
            },
            "prepare",
            1,
        ),
        (
            Opts {
                copy_rc: 1,
                ..opts()
            },
            "copy",
            2,
        ),
        (
            Opts {
                shell: vec![("cp '/opt", 1)],
                ..opts()
            },
            "units",
            3,
        ),
        (
            Opts {
                shell: vec![("podman login", 1)],
                ..opts()
            },
            "binary",
            4,
        ),
    ];
    for (o, failing, count) in cases {
        let (s, log) = service(o);
        let run = s.deploy("a", dir(), false).await.unwrap();
        let got = items(&run);
        assert_eq!(got.len(), count, "{failing}: {got:?}");
        assert!(
            matches!(&got[count - 1], (n, ItemOutcome::Failed(_)) if *n == failing),
            "{got:?}"
        );
        assert_eq!(exit_code(&run), 1);
        assert!(
            !log.all().iter().any(|l| l.contains("./edge up")),
            "{failing}"
        );
    }
}

#[tokio::test]
async fn a_failing_start_fails_the_deploy_and_still_removes_the_env_file() {
    let (s, log) = service(Opts {
        shell: vec![("./edge up", 1)],
        ..opts()
    });
    let run = s.deploy("a", dir(), false).await.unwrap();
    let got = items(&run);
    assert!(
        matches!(&got[4], ("start", ItemOutcome::Failed(m)) if m.contains("boom")),
        "{got:?}"
    );
    assert_eq!(exit_code(&run), 1);
    assert_eq!(
        log.all().last().unwrap(),
        "shell ta [-] rm -f '/dev/shm/acteble-edge.env'"
    );
}

#[tokio::test]
async fn a_verified_target_runs_the_same_steps_without_asking_the_inventory() {
    let (s, log) = service(Opts {
        host: "someone-else",
        ..opts()
    });
    let reports = s.deploy_verified("ta", dir(), false).await.unwrap();
    let got: Vec<_> = reports
        .iter()
        .map(|r| (r.item, r.outcome.clone()))
        .collect();
    assert_eq!(
        got.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        ["prepare", "copy", "units", "binary", "start"]
    );
    assert!(got.iter().all(|(_, o)| *o == ItemOutcome::Applied));
    assert!(log.all()[2].starts_with("shell ta "));
    assert!(log.all().iter().all(|l| !l.contains("docker")));
}

#[tokio::test]
async fn a_verified_target_with_a_missing_file_is_a_validation_error() {
    let (s, log) = service(Opts {
        missing: vec!["/src/edge/Caddyfile".into()],
        ..opts()
    });
    let e = s.deploy_verified("ta", dir(), false).await.unwrap_err();
    assert!(
        matches!(&e, DomainError::ValidationError(m) if m.contains("Caddyfile")),
        "{e:?}"
    );
    assert!(log.all().is_empty());
}
