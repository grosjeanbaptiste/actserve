use std::sync::Arc;

use crate::domain::{
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
};
use crate::ports::outbound::{
    hostname_probe::HostnameProbePort, server_inventory::ServerInventoryPort,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostReport {
    pub name: String,
    pub status: HostStatus,
}

/// True only when the fleet is not empty and every host answered as expected.
pub fn all_ok(reports: &[HostReport]) -> bool {
    !reports.is_empty() && reports.iter().all(|r| r.status == HostStatus::Ok)
}

/// `DomainError`'s Display drops the details of `ExecutionFailed`; ssh's own
/// message (refused, timeout, key) is what the operator needs.
fn describe(e: &DomainError) -> String {
    match e {
        DomainError::ExecutionFailed {
            reason,
            details: Some(d),
        } => format!("{reason}: {d}"),
        other => other.to_string(),
    }
}

pub struct FleetStatusService {
    inventory: Arc<dyn ServerInventoryPort>,
    probe: Arc<dyn HostnameProbePort>,
}

impl FleetStatusService {
    pub fn new(inventory: Arc<dyn ServerInventoryPort>, probe: Arc<dyn HostnameProbePort>) -> Self {
        Self { inventory, probe }
    }

    /// Check every host, or only `name`; an unknown name is `NotFound`.
    pub async fn status(&self, name: Option<&str>) -> Result<Vec<HostReport>, DomainError> {
        let mut hosts = self.inventory.hosts().await?;
        if let Some(wanted) = name {
            hosts.retain(|h| h.name() == wanted);
            if hosts.is_empty() {
                return Err(DomainError::NotFound {
                    entity: "FleetHost",
                    id: wanted.to_string(),
                });
            }
        }
        let mut reports = Vec::with_capacity(hosts.len());
        for host in &hosts {
            reports.push(self.check(host).await);
        }
        Ok(reports)
    }

    async fn check(&self, host: &FleetHost) -> HostReport {
        HostReport {
            name: host.name().to_string(),
            status: probe_status(self.probe.as_ref(), host).await,
        }
    }
}

/// Ask `host` for its hostname and compare it with the inventory.
pub async fn probe_status(probe: &dyn HostnameProbePort, host: &FleetHost) -> HostStatus {
    match probe.hostname(host.ssh_target()).await {
        Err(e) => HostStatus::Unreachable {
            reason: describe(&e),
        },
        Ok(answer) if answer.trim() == host.expected_hostname() => HostStatus::Ok,
        Ok(answer) => HostStatus::WrongHost {
            got: answer.trim().to_string(),
            expected: host.expected_hostname().to_string(),
        },
    }
}

/// One line per host: `name: ok`, `name: unreachable (why)`, `name: wrong-host (answered x, expected y)`.
pub fn render(reports: &[HostReport]) -> String {
    reports
        .iter()
        .map(|r| match &r.status {
            HostStatus::Ok => format!("{}: ok\n", r.name),
            HostStatus::Unreachable { reason } => format!("{}: unreachable ({reason})\n", r.name),
            HostStatus::WrongHost { got, expected } => {
                format!(
                    "{}: wrong-host (answered {got}, expected {expected})\n",
                    r.name
                )
            }
        })
        .collect()
}
