use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::application::harden_service::{HardenRun, exit_code, render};
use actserve::application::runtime_service::RuntimeService;
use actserve::domain::{
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    harden::ItemOutcome,
};
use actserve::ports::outbound::{
    declared_data::DeclaredDataPort,
    hostname_probe::HostnameProbePort,
    remote_shell::{RemoteOutput, RemoteShellPort},
    server_inventory::ServerInventoryPort,
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
    async fn read(&self, _: &str, file: &str) -> Result<String, DomainError> {
        self.0
            .get(file)
            .map(|s| s.to_string())
            .ok_or(DomainError::ExecutionFailed {
                reason: format!("missing {file}"),
                details: None,
            })
    }
}

type Call = (String, Option<String>);
struct Shell {
    rules: Vec<(&'static str, i32)>,
    calls: Mutex<Vec<Call>>,
}
#[async_trait]
impl RemoteShellPort for Shell {
    async fn run(
        &self,
        _: &str,
        c: &str,
        stdin: Option<&str>,
    ) -> Result<RemoteOutput, DomainError> {
        self.calls
            .lock()
            .unwrap()
            .push((c.into(), stdin.map(Into::into)));
        let code = self
            .rules
            .iter()
            .find(|(k, _)| c.contains(k))
            .map_or(0, |r| r.1);
        Ok(RemoteOutput {
            code: Some(code),
            stdout: String::new(),
            stderr: if code == 0 { "".into() } else { "nope".into() },
        })
    }
}

const DIRS: &str = "/opt/a|admin\n";
const JSON: &str = "{}\n";

fn rig(
    probe: &'static str,
    rules: Vec<(&'static str, i32)>,
    dirs: &'static str,
) -> (RuntimeService, Arc<Shell>) {
    let shell = Arc::new(Shell {
        rules,
        calls: Mutex::new(vec![]),
    });
    let data = Data(HashMap::from([
        ("dirs.txt", dirs),
        ("docker-daemon.json", JSON),
    ]));
    let svc = RuntimeService::new(
        Arc::new(Inventory),
        Arc::new(Probe(probe)),
        Arc::new(data),
        shell.clone(),
    );
    (svc, shell)
}

fn outcomes(r: &HardenRun) -> Vec<ItemOutcome> {
    match r {
        HardenRun::Done(i) => i.iter().map(|x| x.outcome.clone()).collect(),
        HardenRun::Refused(s) => panic!("{s:?}"),
    }
}

fn mutating(s: &Shell) -> Vec<String> {
    s.calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.0.clone())
        .filter(|c| c.starts_with("sudo -n sh -c") && !c.contains("stat -c"))
        .collect()
}

#[tokio::test]
async fn a_server_with_the_runtime_in_place_is_checked_and_never_changed() {
    let (svc, shell) = rig("h1", vec![], DIRS);
    let run = svc.run("a", false).await.unwrap();
    assert_eq!(
        outcomes(&run),
        [ItemOutcome::Ok, ItemOutcome::Ok, ItemOutcome::Ok]
    );
    assert!(mutating(&shell).is_empty());
    assert_eq!(exit_code(&run), 0);
}

#[tokio::test]
async fn the_logs_file_is_checked_and_installed_with_the_declared_json_on_stdin() {
    let (svc, shell) = rig("h1", vec![("cmp -s", 1)], DIRS);
    let run = svc.run("a", false).await.unwrap();
    assert_eq!(outcomes(&run)[1], ItemOutcome::Applied);
    let calls = shell.calls.lock().unwrap();
    let check = calls.iter().find(|c| c.0.contains("cmp -s")).unwrap();
    assert_eq!(check.1.as_deref(), Some(JSON));
    let apply = calls
        .iter()
        .find(|c| c.0.contains("daemon.json.new"))
        .unwrap();
    assert_eq!(apply.1.as_deref(), Some(JSON));
}

#[tokio::test]
async fn a_failing_docker_install_stops_before_logs_and_dirs() {
    let (svc, shell) = rig("h1", vec![("compose version", 1), ("apt-get", 100)], DIRS);
    let run = svc.run("a", false).await.unwrap();
    let o = outcomes(&run);
    assert!(
        matches!(&o[0], ItemOutcome::Failed(m) if m.contains("nope")),
        "{o:?}"
    );
    assert_eq!(o.len(), 1);
    assert!(
        !shell
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.0.contains("cmp -s"))
    );
}

#[tokio::test]
async fn directories_with_the_wrong_owner_are_fixed() {
    let (svc, shell) = rig("h1", vec![("stat -c", 1)], DIRS);
    let run = svc.run("a", false).await.unwrap();
    assert_eq!(outcomes(&run)[2], ItemOutcome::Applied);
    assert_eq!(mutating(&shell).len(), 1);
    assert!(mutating(&shell)[0].contains("install -d -o admin -g admin /opt/a"));
}

#[tokio::test]
async fn a_dry_run_applies_nothing() {
    let (svc, shell) = rig(
        "h1",
        vec![("compose version", 1), ("cmp -s", 1), ("stat -c", 1)],
        DIRS,
    );
    let run = svc.run("a", true).await.unwrap();
    assert_eq!(
        outcomes(&run),
        [
            ItemOutcome::WouldApply,
            ItemOutcome::WouldApply,
            ItemOutcome::WouldApply
        ]
    );
    assert!(mutating(&shell).is_empty());
    assert_eq!(exit_code(&run), 0);
}

#[tokio::test]
async fn no_declared_directory_is_ok_without_a_remote_call() {
    let (svc, shell) = rig("h1", vec![], "# none\n");
    let run = svc.run("a", false).await.unwrap();
    assert_eq!(outcomes(&run)[2], ItemOutcome::Ok);
    assert!(
        !shell
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.0.contains("stat -c") || c.0.is_empty())
    );
}

#[tokio::test]
async fn bad_data_or_another_host_means_no_remote_call() {
    let (svc, shell) = rig("h1", vec![], "relative|admin\n");
    assert!(matches!(
        svc.run("a", false).await.unwrap_err(),
        DomainError::ValidationError(_)
    ));
    let (svc2, shell2) = rig("other", vec![], DIRS);
    assert!(matches!(
        svc2.run("a", false).await.unwrap(),
        HardenRun::Refused(HostStatus::WrongHost { .. })
    ));
    assert!(shell.calls.lock().unwrap().is_empty() && shell2.calls.lock().unwrap().is_empty());
    assert!(matches!(
        svc.run("zz", false).await.unwrap_err(),
        DomainError::NotFound { .. }
    ));
}

#[tokio::test]
async fn render_names_the_step_and_notes_when_logs_wait_for_a_restart() {
    let (svc, _) = rig("h1", vec![("cmp -s", 1)], DIRS);
    let run = svc.run("a", false).await.unwrap();
    assert_eq!(
        render("a", &run),
        "runtime docker    ok\nruntime logs      applied\nruntime logs: installed; it takes effect at the next restart of Docker, containers keep running\nruntime dirs      ok\n"
    );
}
