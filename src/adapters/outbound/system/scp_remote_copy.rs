use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use tokio::process::Command;

use super::spawn_retry::retry_busy;
use crate::domain::errors::DomainError;
use crate::ports::outbound::{remote_copy::RemoteCopyPort, remote_shell::RemoteOutput};

/// Runs `scp -o BatchMode=yes -o ConnectTimeout=10 -- <files> <target>:<dir>/`.
pub struct ScpRemoteCopy {
    scp: PathBuf,
}

impl ScpRemoteCopy {
    pub fn new(scp: impl Into<PathBuf>) -> Self {
        Self { scp: scp.into() }
    }
}

#[async_trait]
impl RemoteCopyPort for ScpRemoteCopy {
    async fn missing(&self, files: &[PathBuf]) -> Vec<PathBuf> {
        let mut absent = Vec::new();
        for f in files {
            // A path that cannot be inspected is reported as missing: the
            // deploy refuses it rather than sending a partial set.
            if !tokio::fs::try_exists(f).await.unwrap_or(false) {
                absent.push(f.clone());
            }
        }
        absent
    }

    async fn copy(
        &self,
        ssh_target: &str,
        files: &[PathBuf],
        remote_dir: &str,
    ) -> Result<RemoteOutput, DomainError> {
        let mut cmd = Command::new(&self.scp);
        cmd.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "--"])
            .args(files)
            .arg(format!("{ssh_target}:{remote_dir}/"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let cannot_run = |e: std::io::Error| cannot_run(&self.scp, &e);
        let child = retry_busy(|| std::future::ready(cmd.spawn()))
            .await
            .map_err(cannot_run)?;
        let out = child.wait_with_output().await.map_err(cannot_run)?;
        Ok(RemoteOutput {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

fn cannot_run(scp: &Path, e: &std::io::Error) -> DomainError {
    DomainError::ExecutionFailed {
        reason: format!("Cannot run {}", scp.display()),
        details: Some(e.to_string()),
    }
}
