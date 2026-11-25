use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::application::{
    fleet_status_service::probe_status,
    harden_service::HardenRun,
    item_runner::{describe, failed, find_host, push},
    remote_env::with_remote_env_value,
};
use crate::domain::{
    cdn::{
        CDN_COMPOSE_FILE, CDN_ENV_PATH, CDN_SECRET, geo_script, network_command, prepare_command,
        start_script,
    },
    deploy::{GHCR_SECRET, ghcr_wrapped},
    errors::DomainError,
    fleet::HostStatus,
    harden::{ItemOutcome, ItemReport},
    observe::Stack,
    secret::SecretValue,
};
use crate::ports::outbound::{
    hostname_probe::HostnameProbePort,
    remote_copy::RemoteCopyPort,
    remote_shell::{RemoteOutput, RemoteShellPort},
    secret_store::SecretStorePort,
    server_inventory::ServerInventoryPort,
};

const STEP: &str = "cdn-deploy";

/// Deploys the CDN stack: compose file by copy, shared network, images and
/// stack from the secret store's env on the server's tmpfs, then the geo
/// prefix opened. Same contract as `DeployService`.
pub struct CdnDeployService {
    inventory: Arc<dyn ServerInventoryPort>,
    probe: Arc<dyn HostnameProbePort>,
    shell: Arc<dyn RemoteShellPort>,
    copy: Arc<dyn RemoteCopyPort>,
    secrets: Arc<dyn SecretStorePort>,
}

fn outcome(ran: Result<RemoteOutput, DomainError>) -> ItemOutcome {
    match ran {
        Ok(out) if out.code == Some(0) => ItemOutcome::Applied,
        Ok(out) => ItemOutcome::Failed(failed(&out)),
        Err(e) => ItemOutcome::Failed(describe(&e)),
    }
}

impl CdnDeployService {
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
        cdn_dir: &Path,
        dry_run: bool,
    ) -> Result<HardenRun, DomainError> {
        let host = find_host(&*self.inventory, name).await?;
        let status = probe_status(&*self.probe, &host).await;
        if status != HostStatus::Ok {
            return Ok(HardenRun::Refused(status));
        }
        let items = self
            .deploy_verified(host.ssh_target(), cdn_dir, dry_run)
            .await?;
        Ok(HardenRun::Done(items))
    }

    /// The deploy on a target whose host was already checked by the caller
    /// (`deploy`, or the bootstrap through `ServiceStackDeploy`).
    pub async fn deploy_verified(
        &self,
        target: &str,
        cdn_dir: &Path,
        dry_run: bool,
    ) -> Result<Vec<ItemReport>, DomainError> {
        let file = cdn_dir.join(CDN_COMPOSE_FILE);
        if !self
            .copy
            .missing(std::slice::from_ref(&file))
            .await
            .is_empty()
        {
            return Err(DomainError::ValidationError(format!(
                "cdn-deploy: {} not found (set ACTEBLE_CDN_DIR)",
                file.display()
            )));
        }
        let mut items = Vec::new();
        if dry_run {
            for item in ["prepare", "copy", "network", "start", "geo"] {
                push(&mut items, STEP, item, ItemOutcome::WouldApply);
            }
            return Ok(items);
        }
        self.run(&mut items, target, &file).await;
        Ok(items)
    }

    /// Both secrets are read before the server is touched; stops at the first failure.
    async fn run(&self, items: &mut Vec<ItemReport>, target: &str, file: &PathBuf) {
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
        let copied = self
            .copy
            .copy(target, std::slice::from_ref(file), Stack::Cdn.dir())
            .await;
        if !push(items, STEP, "copy", outcome(copied)) {
            return;
        }
        let network = self.shell.run(target, &network_command(), None).await;
        if !push(items, STEP, "network", outcome(network)) {
            return;
        }
        let start = ghcr_wrapped(&start_script());
        let started = self
            .with_env(target, &env, &start, Some(ghcr.expose()))
            .await;
        if !push(items, STEP, "start", outcome(started)) {
            return;
        }
        let geo = self.with_env(target, &env, &geo_script(), None).await;
        push(items, STEP, "geo", outcome(geo));
    }

    async fn read_secrets(&self) -> Result<(SecretValue, SecretValue), DomainError> {
        let ghcr = self.secrets.get(GHCR_SECRET).await?;
        let env = self.secrets.get(CDN_SECRET).await?;
        Ok((ghcr, env))
    }

    async fn with_env(
        &self,
        target: &str,
        env: &SecretValue,
        script: &str,
        stdin: Option<&str>,
    ) -> Result<RemoteOutput, DomainError> {
        with_remote_env_value(&*self.shell, target, env, CDN_ENV_PATH, script, stdin).await
    }
}
