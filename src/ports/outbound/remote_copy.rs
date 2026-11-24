use std::path::PathBuf;

use async_trait::async_trait;

use crate::domain::errors::DomainError;
use crate::ports::outbound::remote_shell::RemoteOutput;

/// Copies local files to a directory of a remote host.
#[async_trait]
pub trait RemoteCopyPort: Send + Sync {
    /// The files of `files` that do not exist locally.
    async fn missing(&self, files: &[PathBuf]) -> Vec<PathBuf>;

    /// Copies `files` into `remote_dir`. A failing copy is a `code`; an error
    /// means the copy tool could not be started.
    async fn copy(
        &self,
        ssh_target: &str,
        files: &[PathBuf],
        remote_dir: &str,
    ) -> Result<RemoteOutput, DomainError>;
}
