use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::domain::{
    errors::DomainError,
    fleet::{FleetHost, parse_inventory},
};
use crate::ports::outbound::server_inventory::ServerInventoryPort;

/// Reads the `NAME|SSH-TARGET|EXPECTED-HOSTNAME` file kept by actparc
/// (`$DOTFILES_STATE_ROOT/servers.txt`).
pub struct FileServerInventory {
    path: PathBuf,
}

impl FileServerInventory {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

#[async_trait]
impl ServerInventoryPort for FileServerInventory {
    async fn hosts(&self) -> Result<Vec<FleetHost>, DomainError> {
        let text = tokio::fs::read_to_string(&self.path).await.map_err(|e| {
            DomainError::ExecutionFailed {
                reason: format!("Cannot read inventory {}", self.path.display()),
                details: Some(e.to_string()),
            }
        })?;
        parse_inventory(&text)
    }
}
