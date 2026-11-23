use std::path::PathBuf;

use async_trait::async_trait;

use crate::domain::errors::DomainError;
use crate::ports::outbound::declared_data::DeclaredDataPort;

/// `<root>/servers/<server>/<file>`.
pub struct FileDeclaredData {
    root: PathBuf,
}

impl FileDeclaredData {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl DeclaredDataPort for FileDeclaredData {
    async fn read(&self, server: &str, file: &str) -> Result<String, DomainError> {
        let path = self.root.join("servers").join(server).join(file);
        tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| DomainError::ExecutionFailed {
                reason: format!("Cannot read {}", path.display()),
                details: Some(e.to_string()),
            })
    }
}
