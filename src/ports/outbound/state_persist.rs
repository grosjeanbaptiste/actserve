use async_trait::async_trait;

use crate::domain::errors::DomainError;

/// Makes the local state repository durable (commit and push), so a secret
/// changed in it reaches the remote and the other machines.
#[async_trait]
pub trait StatePersistPort: Send + Sync {
    async fn persist(&self) -> Result<(), DomainError>;
}
