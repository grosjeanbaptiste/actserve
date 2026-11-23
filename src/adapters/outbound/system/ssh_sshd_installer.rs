use std::path::PathBuf;

use async_trait::async_trait;

use super::ssh_remote_shell::run_ssh;
use crate::domain::{baseline::SSHD_DROPIN, errors::DomainError, harden::SshdOutcome};
use crate::ports::outbound::{remote_shell::RemoteOutput, sshd_installer::SshdInstallerPort};

/// Installs the sshd drop-in over an ssh ControlMaster connection that stays
/// open: validate with `sshd -t`, reload (never restart), prove access with a
/// fresh connection, and roll back through the held one when the proof fails.
pub struct SshSshdInstaller {
    ssh: PathBuf,
}

fn install_script() -> String {
    let f = SSHD_DROPIN;
    format!(
        "sudo -n sh -c 'set -e; rm -f {f}.bak; cat > {f}.new; [ ! -e {f} ] || cp -p {f} {f}.bak; mv {f}.new {f}; if sshd -t && systemctl reload ssh; then exit 0; fi; if [ -e {f}.bak ]; then mv {f}.bak {f}; else rm -f {f}; fi; exit 1'"
    )
}

fn rollback_script() -> String {
    let f = SSHD_DROPIN;
    format!(
        "sudo -n sh -c ': rollback; if [ -e {f}.bak ]; then mv {f}.bak {f}; else rm -f {f}; fi; systemctl reload ssh'"
    )
}

fn args(opts: &[&str], target: &str, command: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = opts.iter().map(|s| s.to_string()).collect();
    a.push("--".into());
    a.push(target.into());
    a.extend(command.map(String::from));
    a
}

/// A socket path short enough for the unix-socket limit, unique per run.
fn socket_dir() -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    std::path::PathBuf::from(format!("/tmp/acs.{}.{nanos}", std::process::id()))
}

impl SshSshdInstaller {
    pub fn new(ssh: impl Into<PathBuf>) -> Self {
        Self { ssh: ssh.into() }
    }

    async fn held(
        &self,
        sock: &str,
        target: &str,
        command: &str,
        stdin: Option<&str>,
    ) -> Result<RemoteOutput, DomainError> {
        let cp = format!("ControlPath={sock}");
        let a = args(&["-o", "BatchMode=yes", "-o", &cp], target, Some(command));
        run_ssh(&self.ssh, &a, stdin).await
    }

    /// Install, prove with a fresh connection, roll back when it fails.
    async fn install_on(
        &self,
        sock: &str,
        target: &str,
        conf: &str,
    ) -> Result<SshdOutcome, DomainError> {
        let out = self
            .held(sock, target, &install_script(), Some(conf))
            .await?;
        if out.code != Some(0) {
            return Ok(SshdOutcome::Failed {
                reason: format!("install script failed: {}", out.stderr.trim()),
            });
        }
        let fresh = args(
            &["-o", "BatchMode=yes", "-o", "ControlPath=none"],
            target,
            Some("true"),
        );
        if run_ssh(&self.ssh, &fresh, None).await?.code == Some(0) {
            return Ok(SshdOutcome::Installed);
        }
        let back = self.held(sock, target, &rollback_script(), None).await?;
        if back.code != Some(0) {
            return Ok(SshdOutcome::Failed {
                reason: format!(
                    "rollback failed, check {SSHD_DROPIN}: {}",
                    back.stderr.trim()
                ),
            });
        }
        Ok(SshdOutcome::RolledBack)
    }
}

#[async_trait]
impl SshdInstallerPort for SshSshdInstaller {
    async fn install(&self, ssh_target: &str, conf: &str) -> Result<SshdOutcome, DomainError> {
        let dir = socket_dir();
        std::fs::create_dir(&dir).map_err(|e| DomainError::ExecutionFailed {
            reason: format!("Cannot create {}", dir.display()),
            details: Some(e.to_string()),
        })?;
        let sock = dir.join("m").display().to_string();
        let cp = format!("ControlPath={sock}");
        let open = args(
            &[
                "-o",
                "BatchMode=yes",
                "-o",
                "ControlMaster=yes",
                "-o",
                &cp,
                "-o",
                "ControlPersist=60",
                "-fN",
            ],
            ssh_target,
            None,
        );
        let master = run_ssh(&self.ssh, &open, None).await;
        let outcome = match master {
            Ok(m) if m.code == Some(0) => self.install_on(&sock, ssh_target, conf).await,
            Ok(m) => Ok(SshdOutcome::Failed {
                reason: format!("cannot open the held connection: {}", m.stderr.trim()),
            }),
            Err(e) => Err(e),
        };
        self.close(&sock, ssh_target, &dir).await;
        outcome
    }
}

impl SshSshdInstaller {
    /// Best effort: a master that lingers expires with ControlPersist, a
    /// leftover directory is empty. Either is warned about, never silent.
    async fn close(&self, sock: &str, target: &str, dir: &std::path::Path) {
        let cp = format!("ControlPath={sock}");
        let a = args(&["-o", &cp, "-O", "exit"], target, None);
        match run_ssh(&self.ssh, &a, None).await {
            Ok(o) if o.code == Some(0) => {}
            Ok(o) => eprintln!(
                "warn: closing the held connection to {target} failed: {}",
                o.stderr.trim()
            ),
            Err(e) => eprintln!("warn: closing the held connection to {target} failed: {e}"),
        }
        if let Err(e) = std::fs::remove_dir(dir) {
            eprintln!("warn: cannot remove {}: {e}", dir.display());
        }
    }
}
