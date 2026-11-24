use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use async_trait::async_trait;
use tokio::process::Command;

use super::spawn_retry::retry_busy;
use crate::domain::errors::DomainError;
use crate::ports::outbound::caddyfile_source::CaddyfileSourcePort;

/// `git -C <repo> fetch --quiet origin main`, then `git -C <repo> show origin/main:Caddyfile`.
pub struct GitCaddyfileSource {
    git: PathBuf,
}

impl GitCaddyfileSource {
    pub fn new(git: impl Into<PathBuf>) -> Self {
        Self { git: git.into() }
    }

    async fn run(&self, repo: &Path, args: &[&str]) -> Result<Output, DomainError> {
        let mut cmd = Command::new(&self.git);
        cmd.arg("-C")
            .arg(repo)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let cannot_run = |e: std::io::Error| DomainError::ExecutionFailed {
            reason: format!("Cannot run {}", self.git.display()),
            details: Some(e.to_string()),
        };
        let child = retry_busy(|| std::future::ready(cmd.spawn()))
            .await
            .map_err(cannot_run)?;
        child.wait_with_output().await.map_err(cannot_run)
    }
}

fn refused(what: &str, out: &Output) -> DomainError {
    DomainError::ExecutionFailed {
        reason: format!("{what} failed ({})", out.status),
        details: Some(String::from_utf8_lossy(&out.stderr).trim().to_string()),
    }
}

#[async_trait]
impl CaddyfileSourcePort for GitCaddyfileSource {
    async fn origin_main(&self, repo: &Path) -> Result<String, DomainError> {
        let fetch = self
            .run(repo, &["fetch", "--quiet", "origin", "main"])
            .await?;
        if !fetch.status.success() {
            return Err(refused("git fetch origin main", &fetch));
        }
        let show = self.run(repo, &["show", "origin/main:Caddyfile"]).await?;
        if !show.status.success() {
            return Err(refused("git show origin/main:Caddyfile", &show));
        }
        String::from_utf8(show.stdout).map_err(|_| DomainError::ExecutionFailed {
            reason: "the Caddyfile of origin/main is not valid UTF-8".into(),
            details: None,
        })
    }
}
