use std::path::PathBuf;

use async_trait::async_trait;
use tokio::process::Command;

use crate::domain::{
    errors::DomainError,
    fleet_audit::{AuditReport, AuditVerdict},
};
use crate::ports::outbound::edge_audit::EdgeAuditPort;

/// Where the edge binary lives on a server.
// ponytail: one fixed path, per-server stacks move it into servers/<name>/.
const EDGE_AUDIT: &str = "cd '/opt/acteble/edge' && sudo -n ./edge audit";

/// Runs `ssh -o BatchMode=yes -- <target> "cd … && sudo -n ./edge audit"`:
/// exit 0 is conforming, 1 a deviation, any other status is unreadable.
pub struct SshEdgeAudit {
    ssh: PathBuf,
}

impl SshEdgeAudit {
    pub fn new(ssh: impl Into<PathBuf>) -> Self {
        Self { ssh: ssh.into() }
    }
}

#[async_trait]
impl EdgeAuditPort for SshEdgeAudit {
    async fn audit(&self, ssh_target: &str) -> Result<AuditReport, DomainError> {
        let out = Command::new(&self.ssh)
            .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10"])
            .args(["--", ssh_target, EDGE_AUDIT])
            .output()
            .await
            .map_err(|e| DomainError::ExecutionFailed {
                reason: format!("Cannot run {}", self.ssh.display()),
                details: Some(e.to_string()),
            })?;
        let verdict = match out.status.code() {
            Some(0) => AuditVerdict::Conforming,
            Some(1) => AuditVerdict::Deviation,
            _ => AuditVerdict::Unreadable {
                reason: format!(
                    "ssh {ssh_target} failed ({}): {}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            },
        };
        Ok(AuditReport {
            verdict,
            output: String::from_utf8_lossy(&out.stdout).into_owned(),
        })
    }
}
