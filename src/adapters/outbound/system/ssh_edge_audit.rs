use std::{path::PathBuf, sync::Arc};

use actessh::Ssh;
use async_trait::async_trait;

use crate::domain::fleet::AuditVerdict;
use crate::ports::outbound::edge_audit::EdgeAuditPort;

const EDGE_AUDIT: &str = "cd '/opt/acteble/edge' && sudo -n ./edge audit";

/// Runs `edge audit` over actessh: exit 0 conforming, 1 deviation, anything
/// else (ssh failures included) unreadable.
pub struct SshEdgeAudit {
    ssh: Arc<Ssh>,
}

impl SshEdgeAudit {
    pub fn new(ssh: impl Into<PathBuf>) -> Self {
        Self {
            ssh: Arc::new(Ssh::new(ssh)),
        }
    }
}

#[async_trait]
impl EdgeAuditPort for SshEdgeAudit {
    async fn audit(&self, ssh_target: &str) -> AuditVerdict {
        let (ssh, target) = (self.ssh.clone(), ssh_target.to_string());
        let out = match tokio::task::spawn_blocking(move || ssh.exec(&target, EDGE_AUDIT)).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return AuditVerdict::Unreadable(format!("{e:?}")),
            Err(e) => return AuditVerdict::Unreadable(e.to_string()),
        };
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        match out.status.code() {
            Some(0) => AuditVerdict::Conforming(stdout),
            Some(1) => AuditVerdict::Deviation(stdout),
            _ => AuditVerdict::Unreadable(format!(
                "ssh {ssh_target} ended with {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            )),
        }
    }
}
