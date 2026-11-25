use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::application::harden_service::{HardenRun, exit_code, render};
use actserve::application::stacks_service::StacksService;
use actserve::domain::{
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    harden::ItemOutcome,
    stacks::DeployOutcome,
};
use actserve::ports::outbound::{
    declared_data::DeclaredDataPort,
    hostname_probe::HostnameProbePort,
    remote_shell::{RemoteOutput, RemoteShellPort},
    server_inventory::ServerInventoryPort,
    stack_deploy::StackDeployPort,
};

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
struct Data(HashMap<&'static str, &'static str>);
#[async_trait]
impl DeclaredDataPort for Data {
    async fn read(&self, _: &str, f: &str) -> Result<String, DomainError> {
        self.0
            .get(f)
            .map(|s| s.to_string())
            .ok_or(DomainError::ExecutionFailed {
                reason: format!("missing {f}"),
                details: None,
            })
    }
}
struct Shell {
    rules: Vec<(&'static str, i32)>,
    calls: Mutex<Vec<String>>,
}
#[async_trait]
impl RemoteShellPort for Shell {
    async fn run(&self, _: &str, c: &str, _: Option<&str>) -> Result<RemoteOutput, DomainError> {
        self.calls.lock().unwrap().push(c.into());
        let code = self
            .rules
            .iter()
            .find(|(k, _)| c.contains(k))
            .map_or(0, |r| r.1);
        Ok(RemoteOutput {
            code: Some(code),
            stdout: String::new(),
            stderr: "nope".into(),
        })
    }
}
struct Deploy(DeployOutcome, Mutex<Vec<String>>);
#[async_trait]
impl StackDeployPort for Deploy {
    async fn start(&self, t: &str, stack: &str) -> Result<DeployOutcome, DomainError> {
        self.1.lock().unwrap().push(format!("{t}:{stack}"));
        Ok(self.0.clone())
    }
}

struct Rig {
    svc: StacksService,
    shell: Arc<Shell>,
    deploy: Arc<Deploy>,
}

fn rig(
    probe: &'static str,
    rules: Vec<(&'static str, i32)>,
    stacks: &'static str,
    d: DeployOutcome,
) -> Rig {
    let shell = Arc::new(Shell {
        rules,
        calls: Mutex::new(vec![]),
    });
    let deploy = Arc::new(Deploy(d, Mutex::new(vec![])));
    let data = Data(HashMap::from([("stacks.txt", stacks)]));
    let svc = StacksService::new(
        Arc::new(Inventory),
        Arc::new(Probe(probe)),
        Arc::new(data),
        shell.clone(),
        deploy.clone(),
    );
    Rig { svc, shell, deploy }
}

const STACKS: &str = "cdn|run\napi|held\n";

fn outcomes(r: &HardenRun) -> Vec<ItemOutcome> {
    match r {
        HardenRun::Done(i) => i.iter().map(|x| x.outcome.clone()).collect(),
        HardenRun::Refused(s) => panic!("{s:?}"),
    }
}

fn started(r: &Rig) -> Vec<String> {
    r.deploy.1.lock().unwrap().clone()
}

#[tokio::test]
async fn an_edge_that_is_up_is_checked_and_never_touched() {
    let r = rig("h1", vec![], STACKS, DeployOutcome::Started);
    let run = r.svc.edge("a", false).await.unwrap();
    assert_eq!(outcomes(&run), [ItemOutcome::Ok, ItemOutcome::Ok]);
    assert!(started(&r).is_empty());
    assert!(
        !r.shell
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.contains("start acteble_edge-network"))
    );
}

#[tokio::test]
async fn a_missing_network_is_created_then_a_missing_caddy_is_started_by_the_deploy() {
    let r = rig(
        "h1",
        vec![("network exists", 1), ("acteble_edge-caddy", 1)],
        STACKS,
        DeployOutcome::Started,
    );
    let run = r.svc.edge("a", false).await.unwrap();
    assert_eq!(outcomes(&run), [ItemOutcome::Applied, ItemOutcome::Applied]);
    assert_eq!(started(&r), ["ta:edge"]);
    assert!(
        r.shell
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c == "systemctl --user start acteble_edge-network.service")
    );
}

#[tokio::test]
async fn a_deploy_that_cannot_start_is_a_failure_with_its_reason() {
    let r = rig(
        "h1",
        vec![("acteble_edge-caddy", 1)],
        STACKS,
        DeployOutcome::Unavailable("not ported yet".into()),
    );
    let run = r.svc.edge("a", false).await.unwrap();
    assert!(matches!(&outcomes(&run)[1], ItemOutcome::Failed(m) if m == "not ported yet"));
    assert_eq!(exit_code(&run), 1);
}

#[tokio::test]
async fn a_failing_network_creation_stops_before_caddy() {
    let r = rig(
        "h1",
        vec![("network exists", 1), ("start acteble_edge-network", 1)],
        STACKS,
        DeployOutcome::Started,
    );
    let run = r.svc.edge("a", false).await.unwrap();
    let o = outcomes(&run);
    assert_eq!(o.len(), 1);
    assert!(matches!(&o[0], ItemOutcome::Failed(m) if m.contains("nope")));
    assert!(
        !r.shell
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.contains("acteble_edge-caddy"))
    );
}

#[tokio::test]
async fn a_dry_run_starts_nothing() {
    let r = rig(
        "h1",
        vec![
            ("network exists", 1),
            ("acteble_edge-caddy", 1),
            ("acteble_cdn", 1),
        ],
        STACKS,
        DeployOutcome::Started,
    );
    let e = r.svc.edge("a", true).await.unwrap();
    assert_eq!(
        outcomes(&e),
        [ItemOutcome::WouldApply, ItemOutcome::WouldApply]
    );
    let s = r.svc.stacks("a", true).await.unwrap();
    assert_eq!(outcomes(&s), [ItemOutcome::WouldApply, ItemOutcome::Held]);
    assert!(started(&r).is_empty());
    assert!(
        !r.shell
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.contains("start acteble_edge-network"))
    );
}

#[tokio::test]
async fn a_running_stack_is_left_alone_and_a_stopped_one_is_started_in_file_order() {
    let r = rig("h1", vec![], STACKS, DeployOutcome::Started);
    assert_eq!(
        outcomes(&r.svc.stacks("a", false).await.unwrap()),
        [ItemOutcome::Ok, ItemOutcome::Held]
    );
    assert!(started(&r).is_empty());
    let r = rig(
        "h1",
        vec![("acteble_cdn", 1)],
        STACKS,
        DeployOutcome::Started,
    );
    assert_eq!(
        outcomes(&r.svc.stacks("a", false).await.unwrap()),
        [ItemOutcome::Applied, ItemOutcome::Held]
    );
    assert_eq!(started(&r), ["ta:cdn"]);
}

#[tokio::test]
async fn a_held_stack_is_never_checked_or_started() {
    let r = rig("h1", vec![], "api|held\n", DeployOutcome::Started);
    let run = r.svc.stacks("a", false).await.unwrap();
    assert_eq!(outcomes(&run), [ItemOutcome::Held]);
    assert!(r.shell.calls.lock().unwrap().is_empty());
    assert_eq!(render("a", &run), "stacks api       skipped (held back)\n");
}

#[tokio::test]
async fn a_failed_stack_stops_the_ones_after_it() {
    let r = rig(
        "h1",
        vec![("acteble_cdn", 1)],
        "cdn|run\n",
        DeployOutcome::Unavailable("x".into()),
    );
    let run = r.svc.stacks("a", false).await.unwrap();
    assert_eq!(exit_code(&run), 1);
    assert_eq!(render("a", &run), "stacks cdn       fail (x)\n");
}

#[tokio::test]
async fn bad_stacks_another_host_and_unknown_names_make_no_remote_call() {
    let r = rig("h1", vec![], "api|run\n", DeployOutcome::Started);
    assert!(matches!(
        r.svc.stacks("a", false).await.unwrap_err(),
        DomainError::ValidationError(_)
    ));
    assert!(matches!(
        r.svc.stacks("zz", false).await.unwrap_err(),
        DomainError::NotFound { .. }
    ));
    let r2 = rig("other", vec![], STACKS, DeployOutcome::Started);
    assert!(matches!(
        r2.svc.edge("a", false).await.unwrap(),
        HardenRun::Refused(HostStatus::WrongHost { .. })
    ));
    assert!(matches!(
        r2.svc.stacks("a", false).await.unwrap(),
        HardenRun::Refused(_)
    ));
    assert!(r.shell.calls.lock().unwrap().is_empty() && r2.shell.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn edge_render_names_the_step() {
    let r = rig(
        "h1",
        vec![("network exists", 1)],
        STACKS,
        DeployOutcome::Started,
    );
    let run = r.svc.edge("a", false).await.unwrap();
    assert_eq!(
        render("a", &run),
        "edge network   applied\nedge caddy     ok\n"
    );
}
