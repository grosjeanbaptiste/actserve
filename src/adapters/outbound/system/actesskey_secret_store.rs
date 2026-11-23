use std::path::PathBuf;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::process::Command;

use super::spawn_retry::retry_busy;
use crate::domain::{
    errors::DomainError,
    secret::{SecretValue, valid_secret_name},
};
use crate::ports::outbound::secret_store::SecretStorePort;

/// Reads a secret with `actesskey get <name>`: the value is its stdout, verbatim.
pub struct ActesskeySecretStore {
    bin: PathBuf,
}

impl ActesskeySecretStore {
    pub fn new(bin: impl Into<PathBuf>) -> Self {
        Self { bin: bin.into() }
    }
}

#[async_trait]
impl SecretStorePort for ActesskeySecretStore {
    async fn get(&self, name: &str) -> Result<SecretValue, DomainError> {
        if !valid_secret_name(name) {
            return Err(DomainError::ValidationError(format!(
                "invalid secret name {name:?}"
            )));
        }
        let mut cmd = Command::new(&self.bin);
        cmd.args(["get", name])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let cannot_run = |e: std::io::Error| DomainError::ExecutionFailed {
            reason: format!("Cannot run {}", self.bin.display()),
            details: Some(e.to_string()),
        };
        let child = retry_busy(|| std::future::ready(cmd.spawn()))
            .await
            .map_err(cannot_run)?;
        let out = child.wait_with_output().await.map_err(cannot_run)?;
        if !out.status.success() {
            return Err(DomainError::ExecutionFailed {
                reason: format!(
                    "actesskey get {name} exited {}",
                    out.status
                        .code()
                        .map_or("by signal".into(), |c| c.to_string())
                ),
                details: Some(String::from_utf8_lossy(&out.stderr).trim().to_string()),
            });
        }
        if out.stdout.is_empty() {
            return Err(DomainError::ExecutionFailed {
                reason: format!("secret {name} is empty"),
                details: None,
            });
        }
        String::from_utf8(out.stdout)
            .map(SecretValue::new)
            .map_err(|_| DomainError::ExecutionFailed {
                reason: format!("secret {name} is not valid UTF-8"),
                details: None,
            })
    }
}
