use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::application::{
    fleet_status_service::probe_status,
    harden_service::{self, HardenRun},
    item_runner::{failed, find_host},
};
use crate::domain::{
    backup::{BACKUP_SCRIPT, archive_name},
    errors::DomainError,
    fleet::HostStatus,
};
use crate::ports::outbound::{
    hostname_probe::HostnameProbePort, remote_download::RemoteDownloadPort,
    server_inventory::ServerInventoryPort,
};

/// How a backup ended: refused (not the expected host), saved, or failed
/// with nothing left on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupRun {
    Refused(HostStatus),
    Saved(PathBuf),
    Failed(String),
}

/// 0 saved, 1 failed, 3 host refused.
pub fn exit_code(run: &BackupRun) -> u8 {
    match run {
        BackupRun::Saved(_) => 0,
        BackupRun::Failed(_) => 1,
        BackupRun::Refused(_) => 3,
    }
}

pub fn render(name: &str, run: &BackupRun) -> String {
    match run {
        BackupRun::Saved(path) => format!("==> Saved {}\n", path.display()),
        BackupRun::Failed(reason) => {
            format!("edge-backup: transfer failed, nothing saved: {reason}\n")
        }
        BackupRun::Refused(status) => {
            harden_service::render(name, &HardenRun::Refused(status.clone()))
        }
    }
}

/// Saves the certificates (and private keys) of the running edge into a
/// private local archive. The host is checked first.
pub struct EdgeBackupService {
    inventory: Arc<dyn ServerInventoryPort>,
    probe: Arc<dyn HostnameProbePort>,
    download: Arc<dyn RemoteDownloadPort>,
}

impl EdgeBackupService {
    pub fn new(
        inventory: Arc<dyn ServerInventoryPort>,
        probe: Arc<dyn HostnameProbePort>,
        download: Arc<dyn RemoteDownloadPort>,
    ) -> Self {
        Self {
            inventory,
            probe,
            download,
        }
    }

    pub async fn backup(
        &self,
        name: &str,
        dir: &Path,
        stamp: &str,
    ) -> Result<BackupRun, DomainError> {
        let host = find_host(&*self.inventory, name).await?;
        let status = probe_status(&*self.probe, &host).await;
        if status != HostStatus::Ok {
            return Ok(BackupRun::Refused(status));
        }
        let dest = dir.join(archive_name(stamp));
        let ran = self
            .download
            .download(host.ssh_target(), BACKUP_SCRIPT, &dest)
            .await;
        Ok(match ran {
            Ok(out) if out.code == Some(0) => BackupRun::Saved(dest),
            Ok(out) => BackupRun::Failed(failed(&out)),
            Err(DomainError::ExecutionFailed {
                reason,
                details: Some(d),
            }) => BackupRun::Failed(format!("{reason}: {d}")),
            Err(e) => BackupRun::Failed(e.to_string()),
        })
    }
}
