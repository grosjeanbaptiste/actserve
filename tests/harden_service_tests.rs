use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::application::harden_service::{HardenRun, HardenService, exit_code, render};
use actserve::domain::{
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    harden::{ItemOutcome, SshdOutcome},
};
use actserve::ports::outbound::{
    declared_data::DeclaredDataPort,
    hostname_probe::HostnameProbePort,
    remote_shell::{RemoteOutput, RemoteShellPort},
    server_inventory::ServerInventoryPort,
    sshd_installer::SshdInstallerPort,
};

struct Inventory(Vec<FleetHost>);
#[async_trait]
impl ServerInventoryPort for Inventory {
    async fn hosts(&self) -> Result<Vec<FleetHost>, DomainError> {
        Ok(self.0.clone())
    }
}

struct Probe(Result<String, DomainError>);
#[async_trait]
impl HostnameProbePort for Probe {
    async fn hostname(&self, _: &str) -> Result<String, DomainError> {
        self.0.clone()
    }
}

struct Data(HashMap<&'static str, String>);
#[async_trait]
impl DeclaredDataPort for Data {
    async fn read(&self, _server: &str, file: &str) -> Result<String, DomainError> {
        self.0
            .get(file)
            .cloned()
            .ok_or(DomainError::ExecutionFailed {
                reason: format!("missing servers/acteble/{file}"),
                details: None,
            })
    }
}

/// Answers a command with the exit code of the first rule whose text it contains.
struct Shell {
    rules: Vec<(&'static str, i32)>,
    calls: Mutex<Vec<(String, Option<String>)>>,
    broken: bool,
}
#[async_trait]
impl RemoteShellPort for Shell {
    async fn run(
        &self,
        _t: &str,
        command: &str,
        stdin: Option<&str>,
    ) -> Result<RemoteOutput, DomainError> {
        if self.broken {
            return Err(DomainError::ExecutionFailed {
                reason: "Cannot run ssh".into(),
                details: None,
            });
        }
        self.calls
            .lock()
            .unwrap()
            .push((command.to_string(), stdin.map(str::to_string)));
        let code = self
            .rules
            .iter()
            .find(|(k, _)| command.contains(k))
            .map_or(0, |r| r.1);
        Ok(RemoteOutput {
            code: Some(code),
            stdout: String::new(),
            stderr: if code == 0 { "".into() } else { "nope".into() },
        })
    }
}

struct Sshd(SshdOutcome, Mutex<Vec<String>>);
#[async_trait]
impl SshdInstallerPort for Sshd {
    async fn install(&self, _t: &str, conf: &str) -> Result<SshdOutcome, DomainError> {
        self.1.lock().unwrap().push(conf.to_string());
        Ok(self.0.clone())
    }
}

struct Rig {
    svc: HardenService,
    shell: Arc<Shell>,
    sshd: Arc<Sshd>,
}

fn rig(probe: &str, rules: Vec<(&'static str, i32)>, sshd: SshdOutcome, ports: &str) -> Rig {
    let shell = Arc::new(Shell {
        rules,
        calls: Mutex::new(vec![]),
        broken: false,
    });
    rig_with(Probe(Ok(probe.into())), shell, sshd, ports)
}

fn rig_with(probe: Probe, shell: Arc<Shell>, sshd: SshdOutcome, ports: &str) -> Rig {
    let sshd = Arc::new(Sshd(sshd, Mutex::new(vec![])));
    let data = HashMap::from([
        ("sshd.conf", "PermitRootLogin no\n".to_string()),
        ("ports.txt", ports.to_string()),
    ]);
    let svc = HardenService::new(
        Arc::new(Inventory(vec![FleetHost::new("a", "ta", "h1").unwrap()])),
        Arc::new(probe),
        Arc::new(Data(data)),
        shell.clone(),
        sshd.clone(),
    );
    Rig { svc, shell, sshd }
}

const PORTS: &str = "22/tcp\n443/tcp\n";

fn outcomes(run: &HardenRun) -> Vec<ItemOutcome> {
    match run {
        HardenRun::Done(items) => items.iter().map(|i| i.outcome.clone()).collect(),
        HardenRun::Refused(s) => panic!("refused: {s:?}"),
    }
}

fn applied_commands(r: &Rig) -> Vec<String> {
    r.shell
        .calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.0.clone())
        .filter(|c| c.contains("sudo -n sh -c") && !c.contains("grep"))
        .collect()
}

#[tokio::test]
async fn a_server_already_in_its_baseline_is_checked_and_never_changed() {
    let r = rig("h1", vec![], SshdOutcome::Installed, PORTS);
    let run = r.svc.run("a", false).await.unwrap();
    assert_eq!(
        outcomes(&run),
        [
            ItemOutcome::Ok,
            ItemOutcome::Ok,
            ItemOutcome::Ok,
            ItemOutcome::Ok,
            ItemOutcome::Ok
        ]
    );
    assert!(r.sshd.1.lock().unwrap().is_empty());
    assert!(applied_commands(&r).is_empty());
    assert_eq!(exit_code(&run), 0);
}

#[tokio::test]
async fn an_item_failing_its_check_is_applied_and_reported_applied() {
    let r = rig(
        "h1",
        vec![("systemctl is-active", 3)],
        SshdOutcome::Installed,
        PORTS,
    );
    let run = r.svc.run("a", false).await.unwrap();
    assert_eq!(outcomes(&run)[3], ItemOutcome::Applied);
    assert_eq!(
        applied_commands(&r),
        [actserve::domain::baseline::FAIL2BAN_APPLY]
    );
}

#[tokio::test]
async fn the_sshd_drop_in_is_checked_with_the_declared_file_on_stdin_then_installed() {
    let r = rig("h1", vec![("cmp -s", 1)], SshdOutcome::Installed, PORTS);
    let run = r.svc.run("a", false).await.unwrap();
    assert_eq!(outcomes(&run)[1], ItemOutcome::Applied);
    let calls = r.shell.calls.lock().unwrap();
    let check = calls.iter().find(|c| c.0.contains("cmp -s")).unwrap();
    assert_eq!(check.1.as_deref(), Some("PermitRootLogin no\n"));
    assert_eq!(*r.sshd.1.lock().unwrap(), ["PermitRootLogin no\n"]);
}

#[tokio::test]
async fn a_sshd_install_rolled_back_is_a_failure_and_stops_the_run() {
    let r = rig("h1", vec![("cmp -s", 1)], SshdOutcome::RolledBack, PORTS);
    let run = r.svc.run("a", false).await.unwrap();
    let o = outcomes(&run);
    assert!(
        matches!(&o[1], ItemOutcome::Failed(m) if m.contains("rolled back")),
        "{o:?}"
    );
    assert_eq!(o.len(), 2, "nothing after the failed item");
    assert_eq!(exit_code(&run), 1);
}

#[tokio::test]
async fn a_failing_apply_stops_the_run_and_carries_the_remote_error() {
    let r = rig(
        "h1",
        vec![("grep -q", 1), ("ufw default", 1)],
        SshdOutcome::Installed,
        PORTS,
    );
    let run = r.svc.run("a", false).await.unwrap();
    let o = outcomes(&run);
    assert!(
        matches!(&o[2], ItemOutcome::Failed(m) if m.contains("nope")),
        "{o:?}"
    );
    assert_eq!(o.len(), 3);
    assert_eq!(exit_code(&run), 1);
}

#[tokio::test]
async fn a_dry_run_changes_nothing_and_says_what_it_would_apply() {
    let r = rig(
        "h1",
        vec![
            ("cmp -s", 1),
            ("grep -q", 1),
            ("systemctl is-active", 1),
            ("dpkg -s", 1),
        ],
        SshdOutcome::Installed,
        PORTS,
    );
    let run = r.svc.run("a", true).await.unwrap();
    assert_eq!(
        outcomes(&run),
        [
            ItemOutcome::Ok,
            ItemOutcome::WouldApply,
            ItemOutcome::WouldApply,
            ItemOutcome::WouldApply,
            ItemOutcome::WouldApply
        ]
    );
    assert!(r.sshd.1.lock().unwrap().is_empty());
    assert!(applied_commands(&r).is_empty());
    assert_eq!(exit_code(&run), 0);
}

#[tokio::test]
async fn without_a_non_root_user_with_sudo_the_run_stops_at_reach() {
    let r = rig("h1", vec![("id -u", 1)], SshdOutcome::Installed, PORTS);
    let run = r.svc.run("a", false).await.unwrap();
    let o = outcomes(&run);
    assert_eq!(o.len(), 1);
    assert!(matches!(&o[0], ItemOutcome::Failed(m) if m.contains("passwordless sudo")));
}

#[tokio::test]
async fn another_host_is_refused_before_any_command() {
    let r = rig("someone-else", vec![], SshdOutcome::Installed, PORTS);
    let run = r.svc.run("a", false).await.unwrap();
    assert!(matches!(
        run,
        HardenRun::Refused(HostStatus::WrongHost { .. })
    ));
    assert!(r.shell.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn bad_declared_data_is_refused_before_any_command_is_sent() {
    for ports in ["80/tcp\n", "ssh\n22/tcp\n"] {
        let r = rig("h1", vec![], SshdOutcome::Installed, ports);
        let err = r.svc.run("a", false).await.unwrap_err();
        assert!(matches!(err, DomainError::ValidationError(_)), "{err:?}");
        assert!(r.shell.calls.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn a_missing_declared_file_is_an_error_before_any_command() {
    let shell = Arc::new(Shell {
        rules: vec![],
        calls: Mutex::new(vec![]),
        broken: false,
    });
    let mut r = rig_with(
        Probe(Ok("h1".into())),
        shell.clone(),
        SshdOutcome::Installed,
        PORTS,
    );
    r.svc = HardenService::new(
        Arc::new(Inventory(vec![FleetHost::new("a", "ta", "h1").unwrap()])),
        Arc::new(Probe(Ok("h1".into()))),
        Arc::new(Data(HashMap::new())),
        shell.clone(),
        r.sshd.clone(),
    );
    assert!(r.svc.run("a", false).await.is_err());
    assert!(shell.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_unknown_name_is_not_found_and_a_broken_ssh_is_an_error() {
    let r = rig("h1", vec![], SshdOutcome::Installed, PORTS);
    assert!(matches!(
        r.svc.run("nope", false).await.unwrap_err(),
        DomainError::NotFound { .. }
    ));
    let shell = Arc::new(Shell {
        rules: vec![],
        calls: Mutex::new(vec![]),
        broken: true,
    });
    let r = rig_with(Probe(Ok("h1".into())), shell, SshdOutcome::Installed, PORTS);
    assert!(r.svc.run("a", false).await.is_err());
}

#[tokio::test]
async fn render_prints_one_line_per_step_in_the_columns_of_the_bash_tool() {
    let r = rig(
        "h1",
        vec![("systemctl is-active", 1)],
        SshdOutcome::Installed,
        PORTS,
    );
    let run = r.svc.run("a", true).await.unwrap();
    assert_eq!(
        render("a", &run),
        "reach      ok\nharden sshd      ok\nharden ufw       ok\nharden fail2ban  would apply\nharden upgrades  ok\n"
    );
}
