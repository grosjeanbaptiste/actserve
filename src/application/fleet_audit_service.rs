use std::sync::Arc;

use crate::application::fleet_status_service::FleetStatusService;
use crate::domain::{
    errors::DomainError,
    fleet::{AuditVerdict, HostStatus},
};
use crate::ports::outbound::{
    edge_audit::EdgeAuditPort, hostname_probe::HostnameProbePort,
    server_inventory::ServerInventoryPort,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditOutcome {
    /// The host did not answer as expected, so the audit was not run on it.
    NotChecked(HostStatus),
    Audited(AuditVerdict),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditReport {
    pub name: String,
    pub outcome: AuditOutcome,
}

impl AuditReport {
    /// Same codes as `dotfiles servers audit`: 0 conforming, 1 deviation (or
    /// unreachable / another host), 3 unreadable.
    pub fn exit_code(&self) -> u8 {
        match &self.outcome {
            AuditOutcome::Audited(AuditVerdict::Conforming(_)) => 0,
            AuditOutcome::Audited(AuditVerdict::Unreadable(_)) => 3,
            _ => 1,
        }
    }
}

pub struct FleetAuditService {
    status: FleetStatusService,
    audit: Arc<dyn EdgeAuditPort>,
    inventory: Arc<dyn ServerInventoryPort>,
}

impl FleetAuditService {
    pub fn new(
        inventory: Arc<dyn ServerInventoryPort>,
        probe: Arc<dyn HostnameProbePort>,
        audit: Arc<dyn EdgeAuditPort>,
    ) -> Self {
        Self {
            status: FleetStatusService::new(inventory.clone(), probe),
            audit,
            inventory,
        }
    }

    /// Audit one server, after the hostname check; an unknown name is `NotFound`.
    pub async fn audit(&self, name: &str) -> Result<AuditReport, DomainError> {
        let checked = self.status.status(Some(name)).await?;
        let outcome = match checked.into_iter().next().map(|r| r.status) {
            Some(HostStatus::Ok) => {
                let hosts = self.inventory.hosts().await?;
                let host = hosts.iter().find(|h| h.name() == name).ok_or_else(|| {
                    DomainError::NotFound {
                        entity: "FleetHost",
                        id: name.to_string(),
                    }
                })?;
                AuditOutcome::Audited(self.audit.audit(host.ssh_target()).await)
            }
            Some(other) => AuditOutcome::NotChecked(other),
            None => {
                return Err(DomainError::NotFound {
                    entity: "FleetHost",
                    id: name.to_string(),
                });
            }
        };
        Ok(AuditReport {
            name: name.to_string(),
            outcome,
        })
    }
}
