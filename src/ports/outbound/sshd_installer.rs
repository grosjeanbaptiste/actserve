use async_trait::async_trait;

use crate::domain::{errors::DomainError, harden::SshdOutcome};

/// Installs the sshd drop-in safely: validate, reload, prove a fresh
/// connection works, roll back otherwise.
#[async_trait]
pub trait SshdInstallerPort: Send + Sync {
    async fn install(&self, ssh_target: &str, conf: &str) -> Result<SshdOutcome, DomainError>;
}
