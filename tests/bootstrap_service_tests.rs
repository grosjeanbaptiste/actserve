use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::application::{
    bootstrap_service::BootstrapService,
    fleet_audit_service::FleetAuditService,
    harden_service::{HardenRun, HardenService, exit_code, render},
    runtime_service::RuntimeService,
    stacks_service::StacksService,
};
use actserve::domain::{
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    fleet_audit::{AuditReport, AuditVerdict},
    harden::{ItemOutcome, SshdOutcome},
    stacks::DeployOutcome,
};
use actserve::ports::outbound::{
    declared_data::DeclaredDataPort,
    edge_audit::EdgeAuditPort,
    hostname_probe::HostnameProbePort,
    remote_shell::{RemoteOutput, RemoteShellPort},
    server_inventory::ServerInventoryPort,
    sshd_installer::SshdInstallerPort,
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
struct Sshd;
#[async_trait]
impl SshdInstallerPort for Sshd {
    async fn install(&self, _: &str, _: &str) -> Result<SshdOutcome, DomainError> {
        Ok(SshdOutcome::Installed)
    }
}
struct Deploy;
#[async_trait]
impl StackDeployPort for Deploy {
    async fn start(&self, _: &str, _: &str) -> Result<DeployOutcome, DomainError> {
        Ok(DeployOutcome::Started)
    }
}
struct Audit(AuditVerdict, Mutex<u32>);
#[async_trait]
impl EdgeAuditPort for Audit {
    async fn audit(&self, _: &str) -> Result<AuditReport, DomainError> {
        *self.1.lock().unwrap() += 1;
        Ok(AuditReport {
            verdict: self.0.clone(),
            output: String::new(),
        })
    }
}

struct Rig {
    svc: BootstrapService,
    shell: Arc<Shell>,
    audit: Arc<Audit>,
}

const GOOD: [(&str, &str); 6] = [
    ("sshd.conf", "PermitRootLogin no\n"),
    ("ports.txt", "22/tcp\n"),
    ("docker-daemon.json", "{}\n"),
    ("dirs.txt", "/opt/a|admin\n"),
    ("stacks.txt", "cdn|run\napi|held\n"),
    ("unused", ""),
];

fn rig(
    probe: &'static str,
    rules: Vec<(&'static str, i32)>,
    verdict: AuditVerdict,
    patch: &[(&'static str, &'static str)],
) -> Rig {
    let mut files: HashMap<&'static str, &'static str> = GOOD.into_iter().collect();
    for (k, v) in patch {
        if *v == "<absent>" {
            files.remove(k);
        } else {
            files.insert(k, v);
        }
    }
    let data: Arc<Data> = Arc::new(Data(files));
    let shell = Arc::new(Shell {
        rules,
        calls: Mutex::new(vec![]),
    });
    let (inv, pr): (Arc<Inventory>, Arc<Probe>) = (Arc::new(Inventory), Arc::new(Probe(probe)));
    let audit = Arc::new(Audit(verdict, Mutex::new(0)));
    let svc = BootstrapService::new(
        inv.clone(),
        data.clone(),
        Arc::new(HardenService::new(
            inv.clone(),
            pr.clone(),
            data.clone(),
            shell.clone(),
            Arc::new(Sshd),
        )),
        Arc::new(RuntimeService::new(
            inv.clone(),
            pr.clone(),
            data.clone(),
            shell.clone(),
        )),
        Arc::new(StacksService::new(
            inv.clone(),
            pr.clone(),
            data,
            shell.clone(),
            Arc::new(Deploy),
        )),
        Arc::new(FleetAuditService::new(inv, pr, audit.clone())),
    );
    Rig { svc, shell, audit }
}

fn items(r: &HardenRun) -> Vec<(String, ItemOutcome)> {
    match r {
        HardenRun::Done(i) => i
            .iter()
            .map(|x| (format!("{} {}", x.step, x.item), x.outcome.clone()))
            .collect(),
        HardenRun::Refused(s) => panic!("{s:?}"),
    }
}

fn names(r: &HardenRun) -> Vec<String> {
    items(r).into_iter().map(|x| x.0).collect()
}

fn called(r: &Rig, part: &str) -> bool {
    r.shell
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|c| c.contains(part))
}

#[tokio::test]
async fn a_server_in_its_declared_state_passes_all_six_steps_and_exits_0() {
    let r = rig("h1", vec![], AuditVerdict::Conforming, &[]);
    let run = r.svc.run("a", false).await.unwrap();
    assert_eq!(
        names(&run),
        [
            "reach reach",
            "harden sshd",
            "harden ufw",
            "harden fail2ban",
            "harden upgrades",
            "runtime docker",
            "runtime logs",
            "runtime dirs",
            "edge network",
            "edge caddy",
            "stacks cdn",
            "stacks api",
            "audit audit",
        ]
    );
    assert!(
        items(&run)
            .iter()
            .all(|(n, o)| matches!(o, ItemOutcome::Ok) || n == "stacks api")
    );
    assert_eq!(exit_code(&run), 0);
    assert!(render("a", &run).ends_with("stacks api       skipped (held back)\naudit      ok\n"));
}

#[tokio::test]
async fn a_failing_step_stops_the_run_before_the_next_step_and_the_audit() {
    for (rule, last, must_not) in [
        ("id -u", "reach reach", "systemctl is-active"),
        ("systemctl is-active", "harden fail2ban", "compose version"),
        ("compose version", "runtime docker", "network inspect"),
    ] {
        let r = rig(
            "h1",
            vec![(rule, 1), ("apt-get", 1)],
            AuditVerdict::Conforming,
            &[],
        );
        let run = r.svc.run("a", false).await.unwrap();
        let n = names(&run);
        assert_eq!(n.last().unwrap(), last, "{rule}");
        assert_eq!(exit_code(&run), 1);
        assert!(!called(&r, must_not), "{rule}: {must_not}");
        assert_eq!(*r.audit.1.lock().unwrap(), 0);
    }
}

#[tokio::test]
async fn a_failing_network_creation_stops_before_the_audit() {
    let r = rig(
        "h1",
        vec![("network inspect", 1), ("network create", 1)],
        AuditVerdict::Conforming,
        &[],
    );
    let run = r.svc.run("a", false).await.unwrap();
    assert_eq!(names(&run).last().unwrap(), "edge network");
    assert_eq!(*r.audit.1.lock().unwrap(), 0);
}

#[tokio::test]
async fn the_audit_decides_the_exit_status_last() {
    let r = rig("h1", vec![], AuditVerdict::Deviation, &[]);
    let run = r.svc.run("a", false).await.unwrap();
    assert!(render("a", &run).ends_with("audit      fail (deviation)\n"));
    assert_eq!(exit_code(&run), 1);
    let r = rig(
        "h1",
        vec![],
        AuditVerdict::Unreadable {
            reason: "boom".into(),
        },
        &[],
    );
    let run = r.svc.run("a", false).await.unwrap();
    assert!(render("a", &run).ends_with("audit      fail (unreadable: boom)\n"));
    assert_eq!(exit_code(&run), 1);
}

#[tokio::test]
async fn a_dry_run_applies_nothing_and_does_not_audit() {
    let rules = vec![
        ("id -u", 0),
        ("cmp -s", 1),
        ("compose version", 1),
        ("network inspect", 1),
        ("acteble_cdn", 1),
    ];
    let r = rig("h1", rules, AuditVerdict::Deviation, &[]);
    let run = r.svc.run("a", true).await.unwrap();
    assert!(!names(&run).contains(&"audit audit".to_string()));
    assert!(
        items(&run)
            .iter()
            .any(|(_, o)| *o == ItemOutcome::WouldApply)
    );
    assert_eq!(exit_code(&run), 0);
    assert_eq!(*r.audit.1.lock().unwrap(), 0);
    assert!(!called(&r, "network create"));
}

#[tokio::test]
async fn bad_declared_data_anywhere_is_refused_before_the_first_remote_call() {
    for (file, bad) in [
        ("ports.txt", "80/tcp\n"),
        ("dirs.txt", "relative|admin\n"),
        ("stacks.txt", "api|run\n"),
    ] {
        let r = rig("h1", vec![], AuditVerdict::Conforming, &[(file, bad)]);
        let err = r.svc.run("a", false).await.unwrap_err();
        assert!(
            matches!(err, DomainError::ValidationError(_)),
            "{file}: {err:?}"
        );
        assert!(r.shell.calls.lock().unwrap().is_empty(), "{file}");
    }
}

#[tokio::test]
async fn a_missing_file_another_host_and_an_unknown_name_make_no_remote_call() {
    let r = rig(
        "h1",
        vec![],
        AuditVerdict::Conforming,
        &[("docker-daemon.json", "<absent>")],
    );
    assert!(r.svc.run("a", false).await.is_err());
    assert!(matches!(
        r.svc.run("zz", false).await.unwrap_err(),
        DomainError::NotFound { .. }
    ));
    assert!(r.shell.calls.lock().unwrap().is_empty());
    let other = rig("other", vec![], AuditVerdict::Conforming, &[]);
    assert!(matches!(
        other.svc.run("a", false).await.unwrap(),
        HardenRun::Refused(HostStatus::WrongHost { .. })
    ));
    assert!(other.shell.calls.lock().unwrap().is_empty());
}
