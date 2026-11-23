use std::sync::Arc;

use crate::application::{
    fleet_status_service::probe_status,
    harden_service::HardenRun,
    item_runner::{Cmd, check_apply, find_host, push},
};
use crate::domain::{
    dirs_file::parse_dirs, errors::DomainError, fleet::HostStatus, harden::ItemOutcome, runtime,
};
use crate::ports::outbound::{
    declared_data::DeclaredDataPort, hostname_probe::HostnameProbePort,
    remote_shell::RemoteShellPort, server_inventory::ServerInventoryPort,
};

/// Step 3 of the bootstrap: Docker with Compose, bounded container logs, the
/// stack directories. Same contract as `HardenService`.
pub struct RuntimeService {
    inventory: Arc<dyn ServerInventoryPort>,
    probe: Arc<dyn HostnameProbePort>,
    data: Arc<dyn DeclaredDataPort>,
    shell: Arc<dyn RemoteShellPort>,
}

impl RuntimeService {
    pub fn new(
        inventory: Arc<dyn ServerInventoryPort>,
        probe: Arc<dyn HostnameProbePort>,
        data: Arc<dyn DeclaredDataPort>,
        shell: Arc<dyn RemoteShellPort>,
    ) -> Self {
        Self {
            inventory,
            probe,
            data,
            shell,
        }
    }

    /// Declared data is validated before any remote call; the run stops at
    /// the first failed item. With `dry_run` nothing is applied.
    pub async fn run(&self, name: &str, dry_run: bool) -> Result<HardenRun, DomainError> {
        let host = find_host(&*self.inventory, name).await?;
        let json = self.data.read(name, "docker-daemon.json").await?;
        let dirs = parse_dirs("dirs.txt", &self.data.read(name, "dirs.txt").await?)?;
        let status = probe_status(&*self.probe, &host).await;
        if status != HostStatus::Ok {
            return Ok(HardenRun::Refused(status));
        }
        let (t, sh) = (host.ssh_target(), &*self.shell);
        let mut items = Vec::new();
        let docker = check_apply(
            sh,
            t,
            Cmd(runtime::DOCKER_CHECK, None),
            Cmd(runtime::DOCKER_APPLY, None),
            dry_run,
        )
        .await?;
        if !push(&mut items, "runtime", "docker", docker) {
            return Ok(HardenRun::Done(items));
        }
        let logs = check_apply(
            sh,
            t,
            Cmd(runtime::LOGS_CHECK, Some(&json)),
            Cmd(runtime::LOGS_APPLY, Some(&json)),
            dry_run,
        )
        .await?;
        if !push(&mut items, "runtime", "logs", logs) {
            return Ok(HardenRun::Done(items));
        }
        let dirs_out = if dirs.is_empty() {
            ItemOutcome::Ok
        } else {
            let (c, a) = (
                runtime::dirs_check_command(&dirs),
                runtime::dirs_apply_command(&dirs),
            );
            check_apply(sh, t, Cmd(&c, None), Cmd(&a, None), dry_run).await?
        };
        push(&mut items, "runtime", "dirs", dirs_out);
        Ok(HardenRun::Done(items))
    }
}
