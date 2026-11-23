use async_trait::async_trait;

use crate::domain::{errors::DomainError, stacks::DeployOutcome};

/// Starts the edge (`"edge"`) or a stack (`"cdn"`) on a host: files, binary,
/// secrets. An error means it could not even be attempted.
#[async_trait]
pub trait StackDeployPort: Send + Sync {
    async fn start(&self, ssh_target: &str, stack: &str) -> Result<DeployOutcome, DomainError>;
}
