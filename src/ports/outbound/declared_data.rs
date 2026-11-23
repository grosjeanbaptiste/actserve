use async_trait::async_trait;

use crate::domain::errors::DomainError;

/// Reads a declared file of a server (`servers/<name>/<file>`).
#[async_trait]
pub trait DeclaredDataPort: Send + Sync {
    async fn read(&self, server: &str, file: &str) -> Result<String, DomainError>;
}
