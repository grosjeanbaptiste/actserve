use async_trait::async_trait;

use crate::domain::{errors::DomainError, secret::SecretValue};

/// Sets `KEY=VALUE` lines of a text secret in the store that owns it
/// (actesskey): present keys are replaced in place, new ones appended. An error
/// means the secret was not changed; it never carries a value.
#[async_trait]
pub trait SecretWriterPort: Send + Sync {
    async fn set_values(
        &self,
        name: &str,
        values: &[(String, SecretValue)],
    ) -> Result<(), DomainError>;
}
