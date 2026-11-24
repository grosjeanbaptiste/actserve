use std::path::Path;

use async_trait::async_trait;

use crate::domain::errors::DomainError;
use crate::ports::outbound::remote_shell::RemoteOutput;

/// Runs one command on a remote host and writes its stdout, byte for byte, to
/// a new local file `dest`: the file is private (mode 0600), its directory is
/// created or tightened to 0700, an existing file is an error, and when the
/// command fails or cannot run no file is left behind. A failing command is a
/// `code` (stdout stays empty); an error means it could not be attempted.
#[async_trait]
pub trait RemoteDownloadPort: Send + Sync {
    async fn download(
        &self,
        ssh_target: &str,
        command: &str,
        dest: &Path,
    ) -> Result<RemoteOutput, DomainError>;
}
