//! `ServiceStackDeploy`: the real `StackDeployPort`, built on the edge and cdn
//! deploy services with every port faked.
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::application::{
    cdn_deploy_service::CdnDeployService, edge_deploy_service::EdgeDeployService,
    stack_deployer::ServiceStackDeploy,
};
use actserve::domain::{
    errors::DomainError, fleet::FleetHost, secret::SecretValue, stacks::DeployOutcome,
};
use actserve::ports::outbound::{
    hostname_probe::HostnameProbePort,
    remote_copy::RemoteCopyPort,
    remote_shell::{RemoteOutput, RemoteShellPort},
    secret_store::SecretStorePort,
    server_inventory::ServerInventoryPort,
    stack_deploy::StackDeployPort,
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
        panic!("the port is given a verified target: no inventory lookup");
    }
}
struct Probe;
#[async_trait]
impl HostnameProbePort for Probe {
    async fn hostname(&self, _: &str) -> Result<String, DomainError> {
        panic!("the port is given a verified target: no probe");
    }
}
struct Shell(Arc<Log>, Vec<(&'static str, i32)>);
#[async_trait]
impl RemoteShellPort for Shell {
    async fn run(&self, t: &str, c: &str, _: Option<&str>) -> Result<RemoteOutput, DomainError> {
        self.0.push(format!("shell {t} {c}"));
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
struct Copy(Arc<Log>, Vec<PathBuf>);
#[async_trait]
impl RemoteCopyPort for Copy {
    async fn missing(&self, _: &[PathBuf]) -> Vec<PathBuf> {
        self.1.clone()
    }
    async fn copy(&self, t: &str, f: &[PathBuf], d: &str) -> Result<RemoteOutput, DomainError> {
        let names: Vec<_> = f.iter().map(|p| p.display().to_string()).collect();
        self.0.push(format!("copy {t} {} -> {d}", names.join(",")));
        Ok(RemoteOutput {
            code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}
struct Store(bool);
#[async_trait]
impl SecretStorePort for Store {
    async fn get(&self, n: &str) -> Result<SecretValue, DomainError> {
        if self.0 {
            return Err(DomainError::ExecutionFailed {
                reason: format!("{n} locked"),
                details: None,
            });
        }
        Ok(SecretValue::new(format!("value-of-{n}")))
    }
}

fn deployer(
    shell: Vec<(&'static str, i32)>,
    missing: Vec<PathBuf>,
    locked: bool,
) -> (ServiceStackDeploy, Arc<Log>) {
    let log = Arc::new(Log::default());
    let edge = EdgeDeployService::new(
        Arc::new(Inventory),
        Arc::new(Probe),
        Arc::new(Shell(log.clone(), shell.clone())),
        Arc::new(Copy(log.clone(), missing.clone())),
        Arc::new(Store(locked)),
    );
    let cdn = CdnDeployService::new(
        Arc::new(Inventory),
        Arc::new(Probe),
        Arc::new(Shell(log.clone(), shell)),
        Arc::new(Copy(log.clone(), missing)),
        Arc::new(Store(locked)),
    );
    let d = ServiceStackDeploy::new(edge, cdn, Path::new("/src/edge"), Path::new("/src/cdn"));
    (d, log)
}

#[tokio::test]
async fn the_edge_is_started_by_the_edge_deploy() {
    let (d, log) = deployer(vec![], vec![], false);
    assert_eq!(d.start("ta", "edge").await.unwrap(), DeployOutcome::Started);
    let l = log.all();
    assert!(l.iter().any(|c| c.contains("/src/edge/Caddyfile")), "{l:?}");
    assert!(l.iter().any(|c| c.contains("./edge up")));
    assert!(!l.iter().any(|c| c.contains("/src/cdn")));
}

#[tokio::test]
async fn the_cdn_is_started_by_the_cdn_deploy() {
    let (d, log) = deployer(vec![], vec![], false);
    assert_eq!(d.start("ta", "cdn").await.unwrap(), DeployOutcome::Started);
    let l = log.all();
    assert!(
        l.iter()
            .any(|c| c.contains("/src/cdn/quadlet/acteble_cdn-minio.kube")),
        "{l:?}"
    );
    assert!(!l.iter().any(|c| c.contains("/src/edge")));
}

#[tokio::test]
async fn a_failing_step_is_reported_with_its_name_and_reason() {
    let (d, _) = deployer(vec![("./edge up", 1)], vec![], false);
    match d.start("ta", "edge").await.unwrap() {
        DeployOutcome::Unavailable(m) => assert!(m.contains("start") && m.contains("boom"), "{m}"),
        DeployOutcome::Started => panic!("started on a failed step"),
    }
}

#[tokio::test]
async fn a_locked_secret_is_reported_as_unavailable() {
    let (d, log) = deployer(vec![], vec![], true);
    match d.start("ta", "cdn").await.unwrap() {
        DeployOutcome::Unavailable(m) => {
            assert!(m.contains("secrets") && m.contains("locked"), "{m}")
        }
        DeployOutcome::Started => panic!("started with a locked secret"),
    }
    assert!(log.all().is_empty(), "the server must not be touched");
}

#[tokio::test]
async fn a_missing_local_file_is_reported_as_unavailable_with_the_variable_to_set() {
    let (d, log) = deployer(vec![], vec!["/src/cdn/Containerfile".into()], false);
    match d.start("ta", "cdn").await.unwrap() {
        DeployOutcome::Unavailable(m) => assert!(m.contains("ACTEBLE_CDN_DIR"), "{m}"),
        DeployOutcome::Started => panic!("started without its files"),
    }
    assert!(log.all().is_empty());
}

#[tokio::test]
async fn any_other_stack_is_refused_and_named() {
    let (d, log) = deployer(vec![], vec![], false);
    match d.start("ta", "api").await.unwrap() {
        DeployOutcome::Unavailable(m) => {
            assert!(m.contains("api") && m.contains("actserve deploy"), "{m}")
        }
        DeployOutcome::Started => panic!("started an unknown stack"),
    }
    assert!(log.all().is_empty());
}
