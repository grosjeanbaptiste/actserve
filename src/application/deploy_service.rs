use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::application::{
    fleet_status_service::probe_status,
    harden_service::HardenRun,
    item_runner::{failed, find_host, push},
    remote_env::with_remote_env_value,
};
use crate::domain::{
    deploy::{
        API_COMPOSE_FILES, API_ENV_PATH, API_SECRET, GHCR_SECRET, deploy_script, ghcr_wrapped,
        rollback_script,
    },
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    harden::{ItemOutcome, ItemReport},
    observe::Stack,
    secret::SecretValue,
};
use crate::ports::outbound::{
    hostname_probe::HostnameProbePort, remote_copy::RemoteCopyPort, remote_shell::RemoteShellPort,
    secret_store::SecretStorePort, server_inventory::ServerInventoryPort,
};

/// Deploys and rolls back the API on a server: files by copy, the image by
/// compose, the prod env from the secret store on the server's tmpfs for the
/// restart only. Same contract as `HardenService`: stops at the first failure,
/// `dry_run` changes nothing, a host that is not the expected one is refused.
pub struct DeployService {
    inventory: Arc<dyn ServerInventoryPort>,
    probe: Arc<dyn HostnameProbePort>,
    shell: Arc<dyn RemoteShellPort>,
    copy: Arc<dyn RemoteCopyPort>,
    secrets: Arc<dyn SecretStorePort>,
}

fn describe(e: &DomainError) -> String {
    match e {
        DomainError::ExecutionFailed {
            reason,
            details: Some(d),
        } => format!("{reason}: {d}"),
        other => other.to_string(),
    }
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

    /// Copy the compose files and the Caddyfile of `api_dir`, then pull and restart.
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
        let files: Vec<PathBuf> = API_COMPOSE_FILES.iter().map(|f| api_dir.join(f)).collect();
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
            push(&mut items, "deploy", "copy", ItemOutcome::WouldApply);
            push(&mut items, "deploy", "restart", ItemOutcome::WouldApply);
            return Ok(HardenRun::Done(items));
        }
        let Some((ghcr, env)) = self.read_secrets(&mut items, "deploy").await else {
            return Ok(HardenRun::Done(items));
        };
        let copied = self
            .copy
            .copy(host.ssh_target(), &files, Stack::Api.dir())
            .await?;
        let outcome = if copied.code == Some(0) {
            ItemOutcome::Applied
        } else {
            ItemOutcome::Failed(failed(&copied))
        };
        if push(&mut items, "deploy", "copy", outcome) {
            let script = ghcr_wrapped(&deploy_script());
            self.restart(&mut items, "deploy", &host, (&ghcr, &env), &script)
                .await;
        }
        Ok(HardenRun::Done(items))
    }

    /// Restart the API on `tag` (`previous` by default at the CLI).
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
        if let Some(secrets) = self.read_secrets(&mut items, "rollback").await {
            self.restart(
                &mut items,
                "rollback",
                &host,
                (&secrets.0, &secrets.1),
                &script,
            )
            .await;
        }
        Ok(HardenRun::Done(items))
    }

    /// The ghcr token and the prod env, both read before the server is touched.
    async fn read_secrets(
        &self,
        items: &mut Vec<ItemReport>,
        step: &'static str,
    ) -> Option<(SecretValue, SecretValue)> {
        let read = async {
            let ghcr = self.secrets.get(GHCR_SECRET).await?;
            let env = self.secrets.get(API_SECRET).await?;
            Ok::<_, DomainError>((ghcr, env))
        };
        match read.await {
            Ok(both) => Some(both),
            Err(e) => {
                push(items, step, "secrets", ItemOutcome::Failed(describe(&e)));
                None
            }
        }
    }

    async fn restart(
        &self,
        items: &mut Vec<ItemReport>,
        step: &'static str,
        host: &FleetHost,
        (ghcr, env): (&SecretValue, &SecretValue),
        script: &str,
    ) {
        let outcome = match with_remote_env_value(
            &*self.shell,
            host.ssh_target(),
            env,
            API_ENV_PATH,
            script,
            Some(ghcr.expose()),
        )
        .await
        {
            Ok(out) if out.code == Some(0) => ItemOutcome::Applied,
            Ok(out) => ItemOutcome::Failed(failed(&out)),
            Err(e) => ItemOutcome::Failed(describe(&e)),
        };
        push(items, step, "restart", outcome);
    }
}
