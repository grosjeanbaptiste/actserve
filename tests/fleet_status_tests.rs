use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use actserve::application::fleet_status_service::FleetStatusService;
use actserve::domain::{
    errors::DomainError,
    fleet::{FleetHost, HostStatus, parse_inventory},
};
use actserve::ports::outbound::{
    hostname_probe::HostnameProbePort, server_inventory::ServerInventoryPort,
};

struct FixedInventory(Vec<FleetHost>);

#[async_trait]
impl ServerInventoryPort for FixedInventory {
    async fn hosts(&self) -> Result<Vec<FleetHost>, DomainError> {
        Ok(self.0.clone())
    }
}

struct FailingInventory;

#[async_trait]
impl ServerInventoryPort for FailingInventory {
    async fn hosts(&self) -> Result<Vec<FleetHost>, DomainError> {
        Err(DomainError::ExecutionFailed {
            reason: "inventory unreadable".into(),
            details: None,
        })
    }
}

struct FakeProbe(HashMap<String, Result<String, DomainError>>);

#[async_trait]
impl HostnameProbePort for FakeProbe {
    async fn hostname(&self, ssh_target: &str) -> Result<String, DomainError> {
        self.0.get(ssh_target).cloned().expect("unexpected target")
    }
}

fn host(name: &str, target: &str, hostname: &str) -> FleetHost {
    FleetHost::new(name, target, hostname).unwrap()
}

fn service(
    hosts: Vec<FleetHost>,
    probes: Vec<(&str, Result<String, DomainError>)>,
) -> FleetStatusService {
    let probe = FakeProbe(
        probes
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    );
    FleetStatusService::new(Arc::new(FixedInventory(hosts)), Arc::new(probe))
}

fn unreachable_err() -> DomainError {
    DomainError::ExecutionFailed {
        reason: "ssh failed".into(),
        details: Some("Connection refused".into()),
    }
}

#[test]
fn parse_inventory_skips_comments_and_blank_lines() {
    let text =
        "# header\n\nacteble|api.acteble|cloud-1\n  # indented comment\nedge|root@1.2.3.4|edge-1\n";
    let hosts = parse_inventory(text).unwrap();
    assert_eq!(
        hosts,
        vec![
            host("acteble", "api.acteble", "cloud-1"),
            host("edge", "root@1.2.3.4", "edge-1")
        ]
    );
}

#[test]
fn parse_inventory_rejects_wrong_field_count_naming_the_line() {
    let err = parse_inventory("ok|a|b\nbroken|only-two\n").unwrap_err();
    assert!(
        matches!(&err, DomainError::ValidationError(m) if m.contains("line 2")),
        "{err:?}"
    );
}

#[test]
fn parse_inventory_rejects_empty_fields_and_duplicates() {
    assert!(parse_inventory("a||c\n").is_err());
    assert!(parse_inventory("a|b|c|d\n").is_err());
    let err = parse_inventory("a|x|h1\na|y|h2\n").unwrap_err();
    assert!(
        matches!(&err, DomainError::ValidationError(m) if m.contains("duplicate")),
        "{err:?}"
    );
}

#[test]
fn fleet_host_rejects_blank_parts() {
    assert!(FleetHost::new(" ", "t", "h").is_err());
    assert!(FleetHost::new("n", "", "h").is_err());
    assert!(FleetHost::new("n", "t", "  ").is_err());
}

#[tokio::test]
async fn status_is_ok_when_hostname_matches() {
    let svc = service(
        vec![host("a", "ta", "h1")],
        vec![("ta", Ok("h1\n".to_string()))],
    );
    let reports = svc.status(None).await.unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].name, "a");
    assert_eq!(reports[0].status, HostStatus::Ok);
}

#[tokio::test]
async fn status_is_wrong_host_when_another_hostname_answers() {
    let svc = service(
        vec![host("a", "ta", "h1")],
        vec![("ta", Ok("other".to_string()))],
    );
    let reports = svc.status(None).await.unwrap();
    assert_eq!(
        reports[0].status,
        HostStatus::WrongHost {
            got: "other".into()
        }
    );
}

#[tokio::test]
async fn status_is_unreachable_with_the_reason_when_probe_fails() {
    let svc = service(
        vec![host("a", "ta", "h1")],
        vec![("ta", Err(unreachable_err()))],
    );
    let reports = svc.status(None).await.unwrap();
    match &reports[0].status {
        HostStatus::Unreachable { reason } => assert!(reason.contains("Connection refused")),
        other => panic!("expected Unreachable, got {other:?}"),
    }
}

#[tokio::test]
async fn status_by_name_checks_only_that_host_and_unknown_name_is_not_found() {
    let svc = service(
        vec![host("a", "ta", "h1"), host("b", "tb", "h2")],
        vec![("tb", Ok("h2".to_string()))],
    );
    let reports = svc.status(Some("b")).await.unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].name, "b");
    let err = svc.status(Some("zzz")).await.unwrap_err();
    assert!(matches!(err, DomainError::NotFound { .. }));
}

#[tokio::test]
async fn inventory_failure_propagates() {
    let svc = FleetStatusService::new(
        Arc::new(FailingInventory),
        Arc::new(FakeProbe(HashMap::new())),
    );
    assert!(svc.status(None).await.is_err());
}

#[test]
fn all_ok_is_false_when_any_host_is_not_ok_and_for_empty_fleet() {
    use actserve::application::fleet_status_service::{HostReport, all_ok};
    let ok = HostReport {
        name: "a".into(),
        status: HostStatus::Ok,
    };
    let bad = HostReport {
        name: "b".into(),
        status: HostStatus::WrongHost { got: "x".into() },
    };
    assert!(all_ok(std::slice::from_ref(&ok)));
    assert!(!all_ok(&[ok, bad]));
    assert!(!all_ok(&[]));
}

#[test]
fn render_prints_one_line_per_host_with_state_and_detail() {
    use actserve::application::fleet_status_service::{HostReport, render};
    let reports = [
        HostReport {
            name: "a".into(),
            status: HostStatus::Ok,
        },
        HostReport {
            name: "b".into(),
            status: HostStatus::Unreachable {
                reason: "refused".into(),
            },
        },
        HostReport {
            name: "c".into(),
            status: HostStatus::WrongHost { got: "x".into() },
        },
    ];
    assert_eq!(
        render(&reports),
        "a: ok\nb: unreachable (refused)\nc: wrong-host (answered x)\n"
    );
}
