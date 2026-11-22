use std::path::PathBuf;

use async_trait::async_trait;
use tokio::process::Command;

use crate::domain::errors::DomainError;
use crate::ports::outbound::hostname_probe::HostnameProbePort;

/// Runs `ssh -o BatchMode=yes <target> hostname`. The binary is injectable
/// (the bash tool reads `DOTFILES_SSH`) so tests can stand in for ssh.
pub struct SshHostnameProbe {
    ssh: PathBuf,
}

impl SshHostnameProbe {
    pub fn new(ssh: impl Into<PathBuf>) -> Self {
        Self { ssh: ssh.into() }
    }
}

#[async_trait]
impl HostnameProbePort for SshHostnameProbe {
    async fn hostname(&self, ssh_target: &str) -> Result<String, DomainError> {
        let out = Command::new(&self.ssh)
            .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=5"])
            .arg(ssh_target)
            .arg("hostname")
            .output()
            .await
            .map_err(|e| DomainError::ExecutionFailed {
                reason: format!("Cannot run {}", self.ssh.display()),
                details: Some(e.to_string()),
            })?;
        if !out.status.success() {
            return Err(DomainError::ExecutionFailed {
                reason: format!("ssh {ssh_target} failed ({})", out.status),
                details: Some(String::from_utf8_lossy(&out.stderr).trim().to_string()),
            });
        }
        let answer = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if answer.is_empty() {
            return Err(DomainError::ExecutionFailed {
                reason: format!("ssh {ssh_target} returned an empty hostname"),
                details: None,
            });
        }
        Ok(answer)
    }
}
