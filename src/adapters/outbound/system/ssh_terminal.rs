use std::path::PathBuf;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::process::Command;

use super::spawn_retry::retry_busy;
use crate::domain::errors::DomainError;
use crate::ports::outbound::remote_terminal::RemoteTerminalPort;

/// Runs `ssh -t -o BatchMode=yes -o ConnectTimeout=10 -- <target> <command>`
/// with the operator's own stdin, stdout and stderr.
pub struct SshTerminal {
    ssh: PathBuf,
}

impl SshTerminal {
    pub fn new(ssh: impl Into<PathBuf>) -> Self {
        Self { ssh: ssh.into() }
    }
}

#[async_trait]
impl RemoteTerminalPort for SshTerminal {
    async fn attach(&self, ssh_target: &str, command: &str) -> Result<Option<i32>, DomainError> {
        let mut cmd = Command::new(&self.ssh);
        cmd.args(["-t", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "--"])
            .arg(ssh_target)
            .arg(command)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let cannot_run = |e: std::io::Error| DomainError::ExecutionFailed {
            reason: format!("Cannot run {}", self.ssh.display()),
            details: Some(e.to_string()),
        };
        let mut child = retry_busy(|| std::future::ready(cmd.spawn()))
            .await
            .map_err(cannot_run)?;
        let status = child.wait().await.map_err(cannot_run)?;
        Ok(status.code())
    }
}
