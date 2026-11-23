use std::{path::PathBuf, sync::Arc};

use actessh::Ssh;
use async_trait::async_trait;

use crate::domain::errors::DomainError;
use crate::ports::outbound::remote_shell::{RemoteShellPort, ShellOutput};

/// `RemoteShellPort` over actessh.
pub struct SshRemoteShell {
    ssh: Arc<Ssh>,
}

impl SshRemoteShell {
    pub fn new(ssh: impl Into<PathBuf>) -> Self {
        Self {
            ssh: Arc::new(Ssh::new(ssh)),
        }
    }
}

#[async_trait]
impl RemoteShellPort for SshRemoteShell {
    async fn run(
        &self,
        target: &str,
        command: &str,
        stdin: &[u8],
    ) -> Result<ShellOutput, DomainError> {
        let (ssh, target, command, stdin) = (
            self.ssh.clone(),
            target.to_string(),
            command.to_string(),
            stdin.to_vec(),
        );
        let out = tokio::task::spawn_blocking(move || ssh.exec_stdin(&target, &command, &stdin))
            .await
            .map_err(|e| DomainError::ExecutionFailed {
                reason: "ssh did not finish".into(),
                details: Some(e.to_string()),
            })?
            .map_err(|e| DomainError::ExecutionFailed {
                reason: "Cannot run ssh".into(),
                details: Some(format!("{e:?}")),
            })?;
        Ok(ShellOutput {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        })
    }
}
