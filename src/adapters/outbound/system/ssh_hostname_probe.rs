use std::{path::PathBuf, sync::Arc};

use actessh::{Error, Ssh};
use async_trait::async_trait;

use super::spawn_retry::retry_busy;
use crate::domain::errors::DomainError;
use crate::ports::outbound::hostname_probe::HostnameProbePort;

/// Asks `<target> hostname` through actessh (which ends the options with `--`). The binary is injectable
/// (the bash tool reads `DOTFILES_SSH`) so tests can stand in for ssh.
pub struct SshHostnameProbe {
    ssh: Arc<Ssh>,
    bin: PathBuf,
}

impl SshHostnameProbe {
    pub fn new(ssh: impl Into<PathBuf>) -> Self {
        let bin = ssh.into();
        Self {
            ssh: Arc::new(Ssh::new(bin.clone())),
            bin,
        }
    }
}

#[async_trait]
impl HostnameProbePort for SshHostnameProbe {
    async fn hostname(&self, ssh_target: &str) -> Result<String, DomainError> {
        let target = ssh_target.to_string();
        let outcome = retry_busy(|| {
            let (ssh, target) = (self.ssh.clone(), target.clone());
            async move {
                match tokio::task::spawn_blocking(move || ssh.run(&target, "hostname")).await {
                    Ok(Err(Error::Spawn(_, e))) => Err(e),
                    Ok(other) => Ok(other),
                    Err(joined) => Err(std::io::Error::other(joined)),
                }
            }
        })
        .await
        .map_err(|e| DomainError::ExecutionFailed {
            reason: format!("Cannot run {}", self.bin.display()),
            details: Some(e.to_string()),
        })?;
        let answer = outcome.map_err(|e| match e {
            Error::Spawn(bin, e) => DomainError::ExecutionFailed {
                reason: format!("Cannot run {}", bin.display()),
                details: Some(e.to_string()),
            },
            Error::Failed { status, stderr } => DomainError::ExecutionFailed {
                reason: format!("ssh {ssh_target} failed ({status})"),
                details: Some(stderr),
            },
        })?;
        let answer = clean(&answer);
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
