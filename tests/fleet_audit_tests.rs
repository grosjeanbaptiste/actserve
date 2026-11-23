use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::adapters::outbound::system::ssh_edge_audit::SshEdgeAudit;
use actserve::application::fleet_audit_service::{AuditOutcome, FleetAuditService};
use actserve::domain::{
    errors::DomainError,
    fleet::{AuditVerdict, FleetHost, HostStatus},
};
use actserve::ports::outbound::{
    edge_audit::EdgeAuditPort, hostname_probe::HostnameProbePort,
    server_inventory::ServerInventoryPort,
};

struct Inventory(Vec<FleetHost>);

#[async_trait]
impl ServerInventoryPort for Inventory {
    async fn hosts(&self) -> Result<Vec<FleetHost>, DomainError> {
        Ok(self.0.clone())
    }
}

struct Answers(&'static str);

#[async_trait]
impl HostnameProbePort for Answers {
    async fn hostname(&self, _: &str) -> Result<String, DomainError> {
        Ok(self.0.to_string())
    }
}

#[derive(Clone)]
struct Audit {
    verdict: AuditVerdict,
    calls: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl EdgeAuditPort for Audit {
    async fn audit(&self, ssh_target: &str) -> AuditVerdict {
        self.calls.lock().unwrap().push(ssh_target.to_string());
        self.verdict.clone()
    }
}

fn service(answer: &'static str, verdict: AuditVerdict) -> (FleetAuditService, Audit) {
    let audit = Audit {
        verdict,
        calls: Arc::default(),
    };
    let service = FleetAuditService::new(
        Arc::new(Inventory(vec![
            FleetHost::new("acteble", "ops@vps", "cloud-1").unwrap(),
        ])),
        Arc::new(Answers(answer)),
        Arc::new(audit.clone()),
    );
    (service, audit)
}

#[tokio::test]
async fn unknown_server_is_not_found() {
    let (s, audit) = service("cloud-1", AuditVerdict::Conforming(String::new()));
    let err = s.audit("nope").await.unwrap_err();
    assert!(matches!(err, DomainError::NotFound { .. }), "{err:?}");
    assert!(audit.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn conforming_server_exits_0_and_audit_ran_on_its_target() {
    let (s, audit) = service("cloud-1", AuditVerdict::Conforming("all good".into()));
    let report = s.audit("acteble").await.unwrap();
    assert_eq!(report.exit_code(), 0);
    assert_eq!(*audit.calls.lock().unwrap(), vec!["ops@vps".to_string()]);
}

#[tokio::test]
async fn deviation_exits_1_unreadable_exits_3() {
    let (s, _) = service("cloud-1", AuditVerdict::Deviation("ufw off".into()));
    assert_eq!(s.audit("acteble").await.unwrap().exit_code(), 1);
    let (s, _) = service("cloud-1", AuditVerdict::Unreadable("no sudo".into()));
    assert_eq!(s.audit("acteble").await.unwrap().exit_code(), 3);
}

#[tokio::test]
async fn another_host_is_a_deviation_and_the_audit_never_runs_there() {
    let (s, audit) = service("other", AuditVerdict::Conforming(String::new()));
    let report = s.audit("acteble").await.unwrap();
    assert_eq!(report.exit_code(), 1);
    assert!(matches!(
        report.outcome,
        AuditOutcome::NotChecked(HostStatus::WrongHost { .. })
    ));
    assert!(audit.calls.lock().unwrap().is_empty());
}

fn fake_ssh(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("actserve-audit-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ssh");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[tokio::test]
async fn adapter_runs_edge_audit_with_sudo_in_the_edge_dir() {
    let ssh = fake_ssh(
        "args",
        r#"case "$*" in *"-- ops@vps cd '/opt/acteble/edge' && sudo -n ./edge audit") echo "ports ok";; *) echo "bad args: $*" >&2; exit 9;; esac"#,
    );
    let v = SshEdgeAudit::new(ssh).audit("ops@vps").await;
    assert_eq!(v, AuditVerdict::Conforming("ports ok\n".into()));
}

#[tokio::test]
async fn adapter_maps_exit_1_to_deviation_and_anything_else_to_unreadable() {
    let v = SshEdgeAudit::new(fake_ssh("dev", "echo 'ufw off'; exit 1"))
        .audit("t")
        .await;
    assert_eq!(v, AuditVerdict::Deviation("ufw off\n".into()));
    let v = SshEdgeAudit::new(fake_ssh("255", "echo 'refused' >&2; exit 255"))
        .audit("t")
        .await;
    assert!(
        matches!(v, AuditVerdict::Unreadable(ref r) if r.contains("refused")),
        "{v:?}"
    );
}

#[tokio::test]
async fn adapter_missing_binary_is_unreadable_not_conforming() {
    let v = SshEdgeAudit::new("/nonexistent/ssh").audit("t").await;
    assert!(matches!(v, AuditVerdict::Unreadable(_)), "{v:?}");
}
