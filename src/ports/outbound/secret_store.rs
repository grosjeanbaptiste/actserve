use async_trait::async_trait;

use crate::domain::{errors::DomainError, secret::SecretValue};

/// Reads a secret from the store that owns it (actesskey). An error means the
/// secret could not be read; it never carries the value.
#[async_trait]
pub trait SecretStorePort: Send + Sync {
    async fn get(&self, name: &str) -> Result<SecretValue, DomainError>;
}
