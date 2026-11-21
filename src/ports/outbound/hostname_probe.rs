use async_trait::async_trait;

use crate::domain::errors::DomainError;

/// Asks a remote host for its hostname over ssh.
#[async_trait]
pub trait HostnameProbePort: Send + Sync {
    async fn hostname(&self, ssh_target: &str) -> Result<String, DomainError>;
}
