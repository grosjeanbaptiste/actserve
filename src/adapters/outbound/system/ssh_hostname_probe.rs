use std::path::PathBuf;

use async_trait::async_trait;
use tokio::process::Command;

use crate::domain::errors::DomainError;
use crate::ports::outbound::hostname_probe::HostnameProbePort;

/// Runs `ssh -o BatchMode=yes -- <target> hostname` (`--` ends the options, so a
/// target read from the inventory is never taken for one). The binary is injectable
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
            .args(["--", ssh_target, "hostname"])
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
        let answer = clean(&String::from_utf8_lossy(&out.stdout));
        if answer.is_empty() {
            return Err(DomainError::ExecutionFailed {
                reason: format!("ssh {ssh_target} returned an empty hostname"),
                details: None,
            });
        }
        Ok(answer)
    }
}

/// What a server answered, as one printable line: the first line only, no
/// control character, no surrounding space. The answer comes from the network
/// and is printed on the operator's terminal.
fn clean(answer: &str) -> String {
    answer
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .collect::<String>()
        .trim()
        .to_string()
}
