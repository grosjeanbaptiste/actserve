use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::spawn_retry::retry_busy;
use crate::domain::errors::DomainError;
use crate::ports::outbound::remote_shell::{RemoteOutput, RemoteShellPort};

/// Runs `ssh -o BatchMode=yes -o ConnectTimeout=10 -- <target> <command>`.
pub struct SshRemoteShell {
    ssh: PathBuf,
}

impl SshRemoteShell {
    pub fn new(ssh: impl Into<PathBuf>) -> Self {
        Self { ssh: ssh.into() }
    }
}

fn cannot_run(ssh: &Path, e: &std::io::Error) -> DomainError {
    DomainError::ExecutionFailed {
        reason: format!("Cannot run {}", ssh.display()),
        details: Some(e.to_string()),
    }
}

/// Runs `ssh <args>` and collects its answer; `stdin` is fed when given.
pub(super) async fn run_ssh(
    ssh: &Path,
    args: &[String],
    stdin: Option<&str>,
) -> Result<RemoteOutput, DomainError> {
    let mut command = Command::new(ssh);
    command
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = retry_busy(|| std::future::ready(command.spawn()))
        .await
        .map_err(|e| cannot_run(ssh, &e))?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        // A command that exits without reading closes the pipe: its exit
        // code, not the broken pipe, is the answer.
        if let Err(e) = pipe.write_all(text.as_bytes()).await
            && e.kind() != std::io::ErrorKind::BrokenPipe
        {
            return Err(cannot_run(ssh, &e));
        }
    }
    let out = child
        .wait_with_output()
        .await
        .map_err(|e| cannot_run(ssh, &e))?;
    Ok(RemoteOutput {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

#[async_trait]
impl RemoteShellPort for SshRemoteShell {
    async fn run(
        &self,
        ssh_target: &str,
        command: &str,
        stdin: Option<&str>,
    ) -> Result<RemoteOutput, DomainError> {
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
        run_ssh(&self.ssh, &args, stdin).await
    }
}
