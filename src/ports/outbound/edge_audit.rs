use async_trait::async_trait;

use crate::domain::fleet::AuditVerdict;

/// Runs the edge audit on a remote host. It cannot fail: whatever goes wrong
/// is an `Unreadable` verdict, never a pass.
#[async_trait]
pub trait EdgeAuditPort: Send + Sync {
    async fn audit(&self, ssh_target: &str) -> AuditVerdict;
}
