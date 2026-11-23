use async_trait::async_trait;

use crate::domain::errors::DomainError;

/// Runs a command on a remote host attached to the operator's terminal
/// (a pseudo-terminal, output streamed as it comes, Ctrl-C ends it).
/// `Ok(None)` when ssh was killed by a signal; `Err` only when it could not start.
#[async_trait]
pub trait RemoteTerminalPort: Send + Sync {
    async fn attach(&self, ssh_target: &str, command: &str) -> Result<Option<i32>, DomainError>;
}
