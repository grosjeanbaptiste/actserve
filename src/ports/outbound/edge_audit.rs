use async_trait::async_trait;

use crate::domain::{errors::DomainError, fleet_audit::AuditReport};

/// Runs `edge audit` on a remote host. An error means the audit could not be
/// started; a started audit that fails is an `AuditVerdict`.
#[async_trait]
pub trait EdgeAuditPort: Send + Sync {
    async fn audit(&self, ssh_target: &str) -> Result<AuditReport, DomainError>;
}
