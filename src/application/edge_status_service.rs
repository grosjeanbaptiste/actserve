use std::sync::Arc;

use crate::application::{
    fleet_status_service::probe_status, item_runner::find_host, observe_service::ObserveRun,
    remote_env::with_remote_env,
};
use crate::domain::{
    errors::DomainError,
    fleet::HostStatus,
    observe::{EDGE_ENV_PATH, EDGE_SECRET, edge_status_command},
};
use crate::ports::outbound::{
    hostname_probe::HostnameProbePort, remote_shell::RemoteShellPort,
    secret_store::SecretStorePort, server_inventory::ServerInventoryPort,
};

/// What the edge of a server says about itself. The edge reads its config from
/// an env file, so the secret is put on the server's tmpfs for the call only.
pub struct EdgeStatusService {
    inventory: Arc<dyn ServerInventoryPort>,
    probe: Arc<dyn HostnameProbePort>,
    shell: Arc<dyn RemoteShellPort>,
    secrets: Arc<dyn SecretStorePort>,
}

impl EdgeStatusService {
    pub fn new(
        inventory: Arc<dyn ServerInventoryPort>,
        probe: Arc<dyn HostnameProbePort>,
        shell: Arc<dyn RemoteShellPort>,
        secrets: Arc<dyn SecretStorePort>,
    ) -> Self {
        Self {
            inventory,
            probe,
            shell,
            secrets,
        }
    }

    /// Refuses a host that is not the expected one before the secret is read.
    pub async fn status(&self, name: &str) -> Result<ObserveRun, DomainError> {
        let host = find_host(&*self.inventory, name).await?;
        let status = probe_status(&*self.probe, &host).await;
        if status != HostStatus::Ok {
            return Ok(ObserveRun::Refused(status));
        }
        let out = with_remote_env(
            &*self.shell,
            &*self.secrets,
            host.ssh_target(),
            EDGE_SECRET,
            EDGE_ENV_PATH,
            &edge_status_command(),
        )
        .await?;
        Ok(ObserveRun::Output(out))
    }
}
