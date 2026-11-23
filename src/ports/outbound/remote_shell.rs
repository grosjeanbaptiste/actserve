use async_trait::async_trait;

use crate::domain::errors::DomainError;

/// What a remote command answered. `code` is `None` when ssh was killed by a signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl ShellOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// Runs a command on a remote host with `stdin` fed to it. `Err` only when ssh could not run at all.
#[async_trait]
pub trait RemoteShellPort: Send + Sync {
    async fn run(
        &self,
        ssh_target: &str,
        command: &str,
        stdin: &[u8],
    ) -> Result<ShellOutput, DomainError>;
}
