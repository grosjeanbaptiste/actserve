use std::sync::Arc;

use crate::application::fleet_status_service::probe_status;
use crate::domain::{
    errors::DomainError,
    fleet::HostStatus,
    fleet_audit::{AuditRun, AuditVerdict},
};
use crate::ports::outbound::{
    edge_audit::EdgeAuditPort, hostname_probe::HostnameProbePort,
    server_inventory::ServerInventoryPort,
};

/// Audits one server, after checking it is the host the inventory expects.
pub struct FleetAuditService {
    inventory: Arc<dyn ServerInventoryPort>,
    probe: Arc<dyn HostnameProbePort>,
    audit: Arc<dyn EdgeAuditPort>,
}

impl FleetAuditService {
    pub fn new(
        inventory: Arc<dyn ServerInventoryPort>,
        probe: Arc<dyn HostnameProbePort>,
        audit: Arc<dyn EdgeAuditPort>,
    ) -> Self {
        Self {
            inventory,
            probe,
            audit,
        }
    }

    /// An unknown name is `NotFound`; an unreachable or other host is `Refused`.
    pub async fn audit(&self, name: &str) -> Result<AuditRun, DomainError> {
        let hosts = self.inventory.hosts().await?;
        let host =
            hosts
                .iter()
                .find(|h| h.name() == name)
                .ok_or_else(|| DomainError::NotFound {
                    entity: "FleetHost",
                    id: name.to_string(),
                })?;
        match probe_status(self.probe.as_ref(), host).await {
            HostStatus::Ok => Ok(AuditRun::Done(self.audit.audit(host.ssh_target()).await?)),
            refused => Ok(AuditRun::Refused(refused)),
        }
    }
}

/// 0 conforming, 1 deviation or refused, 3 unreadable.
pub fn exit_code(run: &AuditRun) -> u8 {
    match run {
        AuditRun::Refused(_) => 1,
        AuditRun::Done(r) => match r.verdict {
            AuditVerdict::Conforming => 0,
            AuditVerdict::Deviation => 1,
            AuditVerdict::Unreadable { .. } => 3,
        },
    }
}

/// The audit's own output, or one line saying why there is none.
pub fn render(name: &str, run: &AuditRun) -> String {
    match run {
        AuditRun::Refused(status) => crate::application::fleet_status_service::render(&[
            crate::application::fleet_status_service::HostReport {
                name: name.to_string(),
                status: status.clone(),
            },
        ]),
        AuditRun::Done(r) => match &r.verdict {
            AuditVerdict::Unreadable { reason } => format!("{name}: audit unreadable ({reason})\n"),
            _ => r.output.clone(),
        },
    }
}
