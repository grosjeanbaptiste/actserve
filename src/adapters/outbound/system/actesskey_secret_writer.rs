use std::path::PathBuf;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::{io::AsyncWriteExt, process::Command};

use super::spawn_retry::retry_busy;
use crate::domain::{
    errors::DomainError,
    secret::{SecretValue, valid_secret_name},
};
use crate::ports::outbound::secret_writer::SecretWriterPort;

/// Sets keys with `actesskey kv-set <name>`: `KEY<TAB>VALUE` lines on stdin, so
/// no value is ever in an argument.
pub struct ActesskeySecretWriter {
    bin: PathBuf,
}

impl ActesskeySecretWriter {
    pub fn new(bin: impl Into<PathBuf>) -> Self {
        Self { bin: bin.into() }
    }

    fn cannot_run(&self, e: &std::io::Error) -> DomainError {
        DomainError::ExecutionFailed {
            reason: format!("Cannot run {}", self.bin.display()),
            details: Some(e.to_string()),
        }
    }
}

fn refuse(what: &str) -> DomainError {
    DomainError::ValidationError(format!("kv-set: {what}"))
}

/// What `kv-set` reads unambiguously: a secret name, at least one pair, keys
/// without `=`, tab or newline, values without tab or newline.
fn payload(name: &str, values: &[(String, SecretValue)]) -> Result<String, DomainError> {
    if !valid_secret_name(name) {
        return Err(refuse(&format!("invalid secret name {name:?}")));
    }
    if values.is_empty() {
        return Err(refuse("nothing to set"));
    }
    let mut text = String::new();
    for (key, value) in values {
        if key.is_empty() || key.contains(['=', '\t', '\n']) {
            return Err(refuse(&format!("invalid key {key:?}")));
        }
        if value.expose().contains(['\t', '\n']) {
            return Err(refuse(&format!(
                "the value of {key} holds a tab or a newline"
            )));
        }
        text.push_str(&format!("{key}\t{}\n", value.expose()));
    }
    Ok(text)
}

#[async_trait]
impl SecretWriterPort for ActesskeySecretWriter {
    async fn set_values(
        &self,
        name: &str,
        values: &[(String, SecretValue)],
    ) -> Result<(), DomainError> {
        let input = payload(name, values)?;
        let mut cmd = Command::new(&self.bin);
        cmd.args(["kv-set", name])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = retry_busy(|| std::future::ready(cmd.spawn()))
            .await
            .map_err(|e| self.cannot_run(&e))?;
        if let Some(mut stdin) = child.stdin.take() {
            // The tool may exit before reading everything: its status below tells why.
            let sent = stdin.write_all(input.as_bytes()).await;
            drop(stdin);
            if let Err(e) = sent
                && e.kind() != std::io::ErrorKind::BrokenPipe
            {
                return Err(self.cannot_run(&e));
            }
        }
        let out = child
            .wait_with_output()
            .await
            .map_err(|e| self.cannot_run(&e))?;
        if out.status.success() {
            return Ok(());
        }
        let stderr = values.iter().fold(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
            |text, (_, v)| text.replace(v.expose(), "***"),
        );
        Err(DomainError::ExecutionFailed {
            reason: format!(
                "actesskey kv-set {name} exited {}",
                out.status
                    .code()
                    .map_or("by signal".into(), |c| c.to_string())
            ),
            details: Some(stderr),
        })
    }
}
