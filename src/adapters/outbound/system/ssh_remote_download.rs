use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use tokio::process::Command;

use super::spawn_retry::retry_busy;
use crate::domain::errors::DomainError;
use crate::ports::outbound::{remote_download::RemoteDownloadPort, remote_shell::RemoteOutput};

/// Runs `ssh -o BatchMode=yes -o ConnectTimeout=10 -- <target> <command>` with
/// its stdout going straight into a private local file.
pub struct SshRemoteDownload {
    ssh: PathBuf,
}

impl SshRemoteDownload {
    pub fn new(ssh: impl Into<PathBuf>) -> Self {
        Self { ssh: ssh.into() }
    }
}

fn io_error(what: &str, path: &Path, e: &std::io::Error) -> DomainError {
    DomainError::ExecutionFailed {
        reason: format!("{what} {}", path.display()),
        details: Some(e.to_string()),
    }
}

/// The directory of `dest`, created or tightened to 0700.
async fn private_dir(dest: &Path) -> Result<(), DomainError> {
    let dir = dest.parent().unwrap_or_else(|| Path::new("."));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| io_error("Cannot create", dir, &e))?;
    tokio::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .await
        .map_err(|e| io_error("Cannot restrict", dir, &e))
}

/// Starts ssh with stdout on `file` and waits for it.
async fn run(
    ssh: &Path,
    args: &[String],
    file: std::fs::File,
) -> Result<RemoteOutput, DomainError> {
    let mut command = Command::new(ssh);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(file))
        .stderr(Stdio::piped());
    let cannot_run = |e: std::io::Error| io_error("Cannot run", ssh, &e);
    let child = retry_busy(|| std::future::ready(command.spawn()))
        .await
        .map_err(cannot_run)?;
    let out = child.wait_with_output().await.map_err(cannot_run)?;
    Ok(RemoteOutput {
        code: out.status.code(),
        stdout: String::new(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

#[async_trait]
impl RemoteDownloadPort for SshRemoteDownload {
    async fn download(
        &self,
        ssh_target: &str,
        command: &str,
        dest: &Path,
    ) -> Result<RemoteOutput, DomainError> {
        private_dir(dest).await?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dest)
            .map_err(|e| io_error("Cannot create", dest, &e))?;
        let args = [
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=10",
            "--",
            ssh_target,
            command,
        ]
        .map(String::from);
        let ran = run(&self.ssh, &args, file).await;
        if !matches!(&ran, Ok(out) if out.code == Some(0)) {
            tokio::fs::remove_file(dest)
                .await
                .map_err(|e| io_error("Cannot remove", dest, &e))?;
        }
        ran
    }
}
