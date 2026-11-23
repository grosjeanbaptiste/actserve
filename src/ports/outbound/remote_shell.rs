use async_trait::async_trait;

use crate::domain::errors::DomainError;

/// What a remote command answered. `code` is `None` when ssh was killed by a signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Runs one shell command on a remote host, optionally feeding its stdin.
/// An error means ssh could not be started; a failing command is a `code`.
#[async_trait]
pub trait RemoteShellPort: Send + Sync {
    async fn run(
        &self,
        ssh_target: &str,
        command: &str,
        stdin: Option<&str>,
    ) -> Result<RemoteOutput, DomainError>;
}
