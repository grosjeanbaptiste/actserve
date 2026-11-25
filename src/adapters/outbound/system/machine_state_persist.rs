use std::path::PathBuf;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::process::Command;

use super::spawn_retry::retry_busy;
use crate::domain::errors::DomainError;
use crate::ports::outbound::state_persist::StatePersistPort;

/// Commits and pushes the state repository with `machine autocommit`.
pub struct MachineStatePersist {
    bin: PathBuf,
}

impl MachineStatePersist {
    pub fn new(bin: impl Into<PathBuf>) -> Self {
        Self { bin: bin.into() }
    }
}

#[async_trait]
impl StatePersistPort for MachineStatePersist {
    async fn persist(&self) -> Result<(), DomainError> {
        let mut cmd = Command::new(&self.bin);
        cmd.arg("autocommit")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let cannot_run = |e: std::io::Error| DomainError::ExecutionFailed {
            reason: format!("Cannot run {}", self.bin.display()),
            details: Some(e.to_string()),
        };
        let child = retry_busy(|| std::future::ready(cmd.spawn()))
            .await
            .map_err(cannot_run)?;
        let out = child.wait_with_output().await.map_err(cannot_run)?;
        if out.status.success() {
            return Ok(());
        }
        Err(DomainError::ExecutionFailed {
            reason: format!(
                "machine autocommit exited {}",
                out.status
                    .code()
                    .map_or("by signal".into(), |c| c.to_string())
            ),
            details: Some(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        })
    }
}
