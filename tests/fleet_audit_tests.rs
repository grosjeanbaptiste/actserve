use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::adapters::outbound::system::ssh_edge_audit::SshEdgeAudit;
use actserve::application::fleet_audit_service::{FleetAuditService, exit_code, render};
use actserve::domain::{
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    fleet_audit::{AuditReport, AuditRun, AuditVerdict},
};
use actserve::ports::outbound::{
    edge_audit::EdgeAuditPort, hostname_probe::HostnameProbePort,
    server_inventory::ServerInventoryPort,
};

struct FixedInventory(Vec<FleetHost>);

#[async_trait]
impl ServerInventoryPort for FixedInventory {
    async fn hosts(&self) -> Result<Vec<FleetHost>, DomainError> {
        Ok(self.0.clone())
    }
}

struct FakeProbe(HashMap<String, Result<String, DomainError>>);

#[async_trait]
impl HostnameProbePort for FakeProbe {
    async fn hostname(&self, ssh_target: &str) -> Result<String, DomainError> {
        self.0.get(ssh_target).cloned().expect("unexpected target")
    }
}

struct FakeAudit {
    answer: Result<AuditReport, DomainError>,
    calls: Mutex<Vec<String>>,
}

#[async_trait]
impl EdgeAuditPort for FakeAudit {
    async fn audit(&self, ssh_target: &str) -> Result<AuditReport, DomainError> {
        self.calls.lock().unwrap().push(ssh_target.to_string());
        self.answer.clone()
    }
}

fn report(verdict: AuditVerdict, output: &str) -> AuditReport {
    AuditReport {
        verdict,
        output: output.into(),
    }
}

fn setup(
    probe: Result<String, DomainError>,
    answer: Result<AuditReport, DomainError>,
) -> (FleetAuditService, Arc<FakeAudit>) {
    let host = FleetHost::new("a", "ta", "h1").unwrap();
    let audit = Arc::new(FakeAudit {
        answer,
        calls: Mutex::new(vec![]),
    });
    let svc = FleetAuditService::new(
        Arc::new(FixedInventory(vec![host])),
        Arc::new(FakeProbe(HashMap::from([("ta".to_string(), probe)]))),
        audit.clone(),
    );
    (svc, audit)
}

#[tokio::test]
async fn a_host_answering_as_expected_is_audited() {
    let (svc, audit) = setup(
        Ok("h1".into()),
        Ok(report(AuditVerdict::Conforming, "all good\n")),
    );
    let run = svc.audit("a").await.unwrap();
    assert_eq!(
        run,
        AuditRun::Done(report(AuditVerdict::Conforming, "all good\n"))
    );
    assert_eq!(*audit.calls.lock().unwrap(), vec!["ta".to_string()]);
}

#[tokio::test]
async fn a_deviation_is_reported_with_the_audit_output() {
    let (svc, _) = setup(
        Ok("h1".into()),
        Ok(report(AuditVerdict::Deviation, "ufw off\n")),
    );
    let run = svc.audit("a").await.unwrap();
    assert_eq!(
        run,
        AuditRun::Done(report(AuditVerdict::Deviation, "ufw off\n"))
    );
}

#[tokio::test]
async fn another_host_is_refused_and_never_audited() {
    let (svc, audit) = setup(Ok("other".into()), Ok(report(AuditVerdict::Conforming, "")));
    let run = svc.audit("a").await.unwrap();
    assert_eq!(
        run,
        AuditRun::Refused(HostStatus::WrongHost {
            got: "other".into(),
            expected: "h1".into()
        })
    );
    assert!(audit.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_unreachable_host_is_refused_and_never_audited() {
    let err = DomainError::ExecutionFailed {
        reason: "ssh ta failed".into(),
        details: Some("refused".into()),
    };
    let (svc, audit) = setup(Err(err), Ok(report(AuditVerdict::Conforming, "")));
    let run = svc.audit("a").await.unwrap();
    assert!(matches!(
        run,
        AuditRun::Refused(HostStatus::Unreachable { .. })
    ));
    assert!(audit.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_unknown_name_is_not_found() {
    let (svc, _) = setup(Ok("h1".into()), Ok(report(AuditVerdict::Conforming, "")));
    let err = svc.audit("nope").await.unwrap_err();
    assert!(matches!(err, DomainError::NotFound { .. }), "{err:?}");
}

#[tokio::test]
async fn a_failing_audit_port_is_an_error_not_a_verdict() {
    let boom = DomainError::ExecutionFailed {
        reason: "cannot run ssh".into(),
        details: None,
    };
    let (svc, _) = setup(Ok("h1".into()), Err(boom));
    assert!(svc.audit("a").await.is_err());
}

#[test]
fn exit_codes_follow_the_verdict() {
    let done = |v| AuditRun::Done(report(v, ""));
    assert_eq!(exit_code(&done(AuditVerdict::Conforming)), 0);
    assert_eq!(exit_code(&done(AuditVerdict::Deviation)), 1);
    let unreadable = AuditVerdict::Unreadable { reason: "x".into() };
    assert_eq!(exit_code(&done(unreadable)), 3);
    let refused = AuditRun::Refused(HostStatus::Unreachable { reason: "x".into() });
    assert_eq!(exit_code(&refused), 1);
}

#[test]
fn render_prints_the_audit_output_or_why_it_was_refused_or_unreadable() {
    let done = AuditRun::Done(report(AuditVerdict::Deviation, "ufw off\n"));
    assert_eq!(render("a", &done), "ufw off\n");
    let refused = AuditRun::Refused(HostStatus::WrongHost {
        got: "x".into(),
        expected: "h".into(),
    });
    assert_eq!(
        render("a", &refused),
        "a: wrong-host (answered x, expected h)\n"
    );
    let unreadable = AuditRun::Done(report(
        AuditVerdict::Unreadable {
            reason: "ssh failed (exit status: 255): refused".into(),
        },
        "",
    ));
    assert_eq!(
        render("a", &unreadable),
        "a: audit unreadable (ssh failed (exit status: 255): refused)\n"
    );
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("actserve-audit-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fake_ssh(dir: &std::path::Path, body: &str) -> PathBuf {
    let path = dir.join("ssh");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[tokio::test]
async fn the_adapter_runs_edge_audit_with_sudo_after_ending_the_options() {
    let ssh = fake_ssh(
        &scratch("args"),
        r#"case "$*" in *"BatchMode=yes"*" -- t cd '/opt/acteble/edge' && sudo -n ./edge audit --runtime podman") echo conforming;; *) echo "bad args: $*" >&2; exit 9;; esac"#,
    );
    let got = SshEdgeAudit::new(ssh).audit("t").await.unwrap();
    assert_eq!(got, report(AuditVerdict::Conforming, "conforming\n"));
}

#[tokio::test]
async fn the_adapter_maps_exit_1_to_a_deviation_keeping_the_output() {
    let ssh = fake_ssh(&scratch("dev"), "echo 'ufw off'; exit 1");
    let got = SshEdgeAudit::new(ssh).audit("t").await.unwrap();
    assert_eq!(got, report(AuditVerdict::Deviation, "ufw off\n"));
}

#[tokio::test]
async fn the_adapter_maps_any_other_status_to_unreadable_with_stderr() {
    for code in [2, 255] {
        let ssh = fake_ssh(
            &scratch(&format!("unr{code}")),
            &format!("echo 'Connection refused' >&2; exit {code}"),
        );
        let got = SshEdgeAudit::new(ssh).audit("t").await.unwrap();
        assert!(
            matches!(&got.verdict, AuditVerdict::Unreadable { reason } if reason.contains("Connection refused")),
            "{code}: {got:?}"
        );
    }
}

#[tokio::test]
async fn the_adapter_with_a_missing_binary_is_an_error_not_a_verdict() {
    let missing = scratch("missing").join("no-such-ssh");
    let err = SshEdgeAudit::new(missing).audit("t").await.unwrap_err();
    assert!(
        matches!(err, DomainError::ExecutionFailed { .. }),
        "{err:?}"
    );
}
