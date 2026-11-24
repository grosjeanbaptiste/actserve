use std::path::Path;

use async_trait::async_trait;

use crate::domain::errors::DomainError;

/// Where the Caddyfile to deploy comes from: the default branch of the API
/// repository, never its working tree.
#[async_trait]
pub trait CaddyfileSourcePort: Send + Sync {
    /// The `Caddyfile` of `origin/main` in the repository at `repo`, after a fetch.
    async fn origin_main(&self, repo: &Path) -> Result<String, DomainError>;
}
