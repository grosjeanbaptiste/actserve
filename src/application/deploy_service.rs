use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::application::{
    fleet_status_service::probe_status,
    harden_service::HardenRun,
    item_runner::{describe, find_host, outcome, push},
    remote_env::with_remote_env_value,
};
use crate::domain::{
    api_podman::{
        API_UNIT_DIR, API_UNIT_FILES, missing_env_keys, prepare_command, rollback_script,
        start_script,
    },
    deploy::{API_ENV_PATH, API_SECRET, GHCR_SECRET},
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    harden::{ItemOutcome, ItemReport},
    podman::ghcr_wrapped,
    secret::SecretValue,
};
use crate::ports::outbound::{
    hostname_probe::HostnameProbePort, remote_copy::RemoteCopyPort, remote_shell::RemoteShellPort,
    secret_store::SecretStorePort, server_inventory::ServerInventoryPort,
};

/// Deploys and rolls back the API on a server: the Quadlet units by copy, the image by Podman,
/// the prod env from the secret store on the server's tmpfs for the restart only (its values go
/// into Podman secrets). Same contract as `HardenService`: stops at the first failure, `dry_run`
/// changes nothing, a host that is not the expected one is refused.
pub struct DeployService {
    inventory: Arc<dyn ServerInventoryPort>,
    probe: Arc<dyn HostnameProbePort>,
    shell: Arc<dyn RemoteShellPort>,
    copy: Arc<dyn RemoteCopyPort>,
    secrets: Arc<dyn SecretStorePort>,
}

impl DeployService {
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

    async fn expected_host(
        &self,
        name: &str,
    ) -> Result<Result<FleetHost, HostStatus>, DomainError> {
        let host = find_host(&*self.inventory, name).await?;
        Ok(match probe_status(&*self.probe, &host).await {
            HostStatus::Ok => Ok(host),
            other => Err(other),
        })
    }

    /// Copy the Quadlet units of `api_dir`, then pull, create the secrets and restart.
    pub async fn deploy(
        &self,
        name: &str,
        api_dir: &Path,
        dry_run: bool,
    ) -> Result<HardenRun, DomainError> {
        let host = match self.expected_host(name).await? {
            Ok(host) => host,
            Err(status) => return Ok(HardenRun::Refused(status)),
        };
        let files: Vec<PathBuf> = API_UNIT_FILES.iter().map(|f| api_dir.join(f)).collect();
        let missing = self.copy.missing(&files).await;
        if !missing.is_empty() {
            let list: Vec<_> = missing.iter().map(|p| p.display().to_string()).collect();
            return Err(DomainError::ValidationError(format!(
                "deploy: not found (set ACTEBLE_API_DIR): {}",
                list.join(", ")
            )));
        }
        let mut items = Vec::new();
        if dry_run {
            for item in ["prepare", "copy", "restart"] {
                push(&mut items, "deploy", item, ItemOutcome::WouldApply);
            }
        } else {
            self.run_deploy(&mut items, &host, &files).await;
        }
        Ok(HardenRun::Done(items))
    }

    /// Both secrets are read, and the env checked, before the server is touched.
    async fn run_deploy(&self, items: &mut Vec<ItemReport>, host: &FleetHost, files: &[PathBuf]) {
        let Some((ghcr, env)) = self.read_secrets(items).await else {
            return;
        };
        let keys = missing_env_keys(env.expose());
        if !keys.is_empty() {
            let reason = format!("missing from {API_SECRET}: {}", keys.join(", "));
            push(items, "deploy", "secrets", ItemOutcome::Failed(reason));
            return;
        }
        let target = host.ssh_target();
        let prepared = self.shell.run(target, &prepare_command(), None).await;
        if !push(items, "deploy", "prepare", outcome(prepared)) {
            return;
        }
        let copied = self.copy.copy(target, files, API_UNIT_DIR).await;
        if !push(items, "deploy", "copy", outcome(copied)) {
            return;
        }
        let script = ghcr_wrapped(&start_script());
        let started = with_remote_env_value(
            &*self.shell,
            target,
            &env,
            API_ENV_PATH,
            &script,
            Some(ghcr.expose()),
        )
        .await;
        push(items, "deploy", "restart", outcome(started));
    }

    /// Restart the API on `tag` (`previous` by default at the CLI). Needs the ghcr token only:
    /// the secrets and settings of the last deploy stay in Podman.
    pub async fn rollback(
        &self,
        name: &str,
        tag: &str,
        dry_run: bool,
    ) -> Result<HardenRun, DomainError> {
        let script = ghcr_wrapped(&rollback_script(tag)?);
        let host = match self.expected_host(name).await? {
            Ok(host) => host,
            Err(status) => return Ok(HardenRun::Refused(status)),
        };
        let mut items = Vec::new();
        if dry_run {
            push(&mut items, "rollback", "restart", ItemOutcome::WouldApply);
            return Ok(HardenRun::Done(items));
        }
        match self.secrets.get(GHCR_SECRET).await {
            Ok(ghcr) => {
                let ran = self
                    .shell
                    .run(host.ssh_target(), &script, Some(ghcr.expose()))
                    .await;
                push(&mut items, "rollback", "restart", outcome(ran));
            }
            Err(e) => {
                push(
                    &mut items,
                    "rollback",
                    "secrets",
                    ItemOutcome::Failed(describe(&e)),
                );
            }
        }
        Ok(HardenRun::Done(items))
    }

    /// The ghcr token and the prod env, both read before the server is touched.
    async fn read_secrets(
        &self,
        items: &mut Vec<ItemReport>,
    ) -> Option<(SecretValue, SecretValue)> {
        let read = async {
            let ghcr = self.secrets.get(GHCR_SECRET).await?;
            let env = self.secrets.get(API_SECRET).await?;
            Ok::<_, DomainError>((ghcr, env))
        };
        match read.await {
            Ok(both) => Some(both),
            Err(e) => {
                push(
                    items,
                    "deploy",
                    "secrets",
                    ItemOutcome::Failed(describe(&e)),
                );
                None
            }
        }
    }
}
