use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::application::{
    fleet_status_service::probe_status,
    harden_service::HardenRun,
    item_runner::{describe, find_host, outcome, push},
    remote_env::with_remote_env_value,
};
use crate::domain::{
    deploy::GHCR_SECRET,
    edge_deploy::{
        EDGE_FILES, EDGE_SECRET, binary_script, prepare_command, start_command, units_command,
    },
    errors::DomainError,
    fleet::HostStatus,
    harden::{ItemOutcome, ItemReport},
    observe::{EDGE_DIR, EDGE_ENV_PATH},
    podman::ghcr_wrapped,
    secret::SecretValue,
};
use crate::ports::outbound::{
    hostname_probe::HostnameProbePort, remote_copy::RemoteCopyPort, remote_shell::RemoteShellPort,
    secret_store::SecretStorePort, server_inventory::ServerInventoryPort,
};

const STEP: &str = "edge-deploy";

/// Deploys the edge: its files by copy, its binary taken out of the image, then
/// started with its config from the secret store on the server's tmpfs.
/// Same contract as `DeployService`.
pub struct EdgeDeployService {
    inventory: Arc<dyn ServerInventoryPort>,
    probe: Arc<dyn HostnameProbePort>,
    shell: Arc<dyn RemoteShellPort>,
    copy: Arc<dyn RemoteCopyPort>,
    secrets: Arc<dyn SecretStorePort>,
}

impl EdgeDeployService {
    pub fn new(
        inventory: Arc<dyn ServerInventoryPort>,
        probe: Arc<dyn HostnameProbePort>,
        shell: Arc<dyn RemoteShellPort>,
        copy: Arc<dyn RemoteCopyPort>,
        secrets: Arc<dyn SecretStorePort>,
    ) -> Self {
        Self {
            inventory,
            probe,
            shell,
            copy,
            secrets,
        }
    }

    pub async fn deploy(
        &self,
        name: &str,
        edge_dir: &Path,
        dry_run: bool,
    ) -> Result<HardenRun, DomainError> {
        let host = find_host(&*self.inventory, name).await?;
        let status = probe_status(&*self.probe, &host).await;
        if status != HostStatus::Ok {
            return Ok(HardenRun::Refused(status));
        }
        let items = self
            .deploy_verified(host.ssh_target(), edge_dir, dry_run)
            .await?;
        Ok(HardenRun::Done(items))
    }

    /// The deploy on a target whose host was already checked by the caller
    /// (`deploy`, or the bootstrap through `ServiceStackDeploy`).
    pub async fn deploy_verified(
        &self,
        target: &str,
        edge_dir: &Path,
        dry_run: bool,
    ) -> Result<Vec<ItemReport>, DomainError> {
        let files: Vec<PathBuf> = EDGE_FILES.iter().map(|f| edge_dir.join(f)).collect();
        let missing = self.copy.missing(&files).await;
        if !missing.is_empty() {
            let list: Vec<_> = missing.iter().map(|p| p.display().to_string()).collect();
            return Err(DomainError::ValidationError(format!(
                "edge-deploy: not found (set ACTEBLE_EDGE_DIR): {}",
                list.join(", ")
            )));
        }
        let mut items = Vec::new();
        if dry_run {
            for item in ["prepare", "copy", "units", "binary", "start"] {
                push(&mut items, STEP, item, ItemOutcome::WouldApply);
            }
            return Ok(items);
        }
        self.run(&mut items, target, &files).await;
        Ok(items)
    }

    /// Both secrets are read before the server is touched; stops at the first failure.
    async fn run(&self, items: &mut Vec<ItemReport>, target: &str, files: &[PathBuf]) {
        let (ghcr, env) = match self.read_secrets().await {
            Ok(both) => both,
            Err(e) => {
                push(items, STEP, "secrets", ItemOutcome::Failed(describe(&e)));
                return;
            }
        };
        let prepared = self.shell.run(target, &prepare_command(), None).await;
        if !push(items, STEP, "prepare", outcome(prepared)) {
            return;
        }
        let copied = self.copy.copy(target, files, EDGE_DIR).await;
        if !push(items, STEP, "copy", outcome(copied)) {
            return;
        }
        let installed = self.shell.run(target, &units_command(), None).await;
        if !push(items, STEP, "units", outcome(installed)) {
            return;
        }
        let binary = ghcr_wrapped(&binary_script());
        let taken = self.shell.run(target, &binary, Some(ghcr.expose())).await;
        if !push(items, STEP, "binary", outcome(taken)) {
            return;
        }
        let started = with_remote_env_value(
            &*self.shell,
            target,
            &env,
            EDGE_ENV_PATH,
            &start_command(),
            None,
        )
        .await;
        push(items, STEP, "start", outcome(started));
    }

    async fn read_secrets(&self) -> Result<(SecretValue, SecretValue), DomainError> {
        let ghcr = self.secrets.get(GHCR_SECRET).await?;
        let env = self.secrets.get(EDGE_SECRET).await?;
        Ok((ghcr, env))
    }
}
