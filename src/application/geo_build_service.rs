use std::sync::Arc;

use crate::application::{
    fleet_status_service::probe_status,
    harden_service::HardenRun,
    item_runner::{describe, failed, find_host, push},
    remote_env::with_remote_env_value,
};
use crate::domain::{
    cdn::{CDN_ENV_PATH, CDN_SECRET},
    deploy::{GHCR_SECRET, ghcr_wrapped},
    errors::DomainError,
    fleet::HostStatus,
    geo_build::{GeoStage, SIGNING_KEY, build_script, env_has_value, follow_command},
    harden::{ItemOutcome, ItemReport},
    secret::SecretValue,
};
use crate::ports::outbound::{
    hostname_probe::HostnameProbePort, remote_shell::RemoteShellPort,
    remote_terminal::RemoteTerminalPort, secret_store::SecretStorePort,
    server_inventory::ServerInventoryPort,
};

const STEP: &str = "cdn-build-geo";

/// Starts the geo build on the CDN host (detached, env on the server's tmpfs,
/// removed after) and follows it to its end. Same contract as `DeployService`:
/// host first, secrets read (and the signing key checked) before the server is
/// touched, `--dry-run` reads and runs nothing, stop at the first failure.
pub struct GeoBuildService {
    inventory: Arc<dyn ServerInventoryPort>,
    probe: Arc<dyn HostnameProbePort>,
    shell: Arc<dyn RemoteShellPort>,
    terminal: Arc<dyn RemoteTerminalPort>,
    secrets: Arc<dyn SecretStorePort>,
}

impl GeoBuildService {
    pub fn new(
        inventory: Arc<dyn ServerInventoryPort>,
        probe: Arc<dyn HostnameProbePort>,
        shell: Arc<dyn RemoteShellPort>,
        terminal: Arc<dyn RemoteTerminalPort>,
        secrets: Arc<dyn SecretStorePort>,
    ) -> Self {
        Self {
            inventory,
            probe,
            shell,
            terminal,
            secrets,
        }
    }

    pub async fn build(
        &self,
        name: &str,
        stage: GeoStage,
        dry_run: bool,
    ) -> Result<HardenRun, DomainError> {
        let host = find_host(&*self.inventory, name).await?;
        let status = probe_status(&*self.probe, &host).await;
        if status != HostStatus::Ok {
            return Ok(HardenRun::Refused(status));
        }
        let mut items = Vec::new();
        if dry_run {
            for item in ["start", "follow"] {
                push(&mut items, STEP, item, ItemOutcome::WouldApply);
            }
        } else {
            self.run(&mut items, host.ssh_target(), stage).await;
        }
        Ok(HardenRun::Done(items))
    }

    async fn run(&self, items: &mut Vec<ItemReport>, target: &str, stage: GeoStage) {
        let (ghcr, env) = match self.read_secrets().await {
            Ok(both) => both,
            Err(e) => {
                push(items, STEP, "secrets", ItemOutcome::Failed(describe(&e)));
                return;
            }
        };
        let script = ghcr_wrapped(&build_script(stage));
        let started = with_remote_env_value(
            &*self.shell,
            target,
            &env,
            CDN_ENV_PATH,
            &script,
            Some(ghcr.expose()),
        )
        .await;
        let outcome = match started {
            Ok(out) if out.code == Some(0) => ItemOutcome::Applied,
            Ok(out) => ItemOutcome::Failed(failed(&out)),
            Err(e) => ItemOutcome::Failed(describe(&e)),
        };
        if !push(items, STEP, "start", outcome) {
            return;
        }
        let followed = match self.terminal.attach(target, &follow_command()).await {
            Ok(Some(0)) => ItemOutcome::Applied,
            Ok(Some(code)) => ItemOutcome::Failed(format!("exit {code}")),
            Ok(None) => ItemOutcome::Failed("ended by a signal".to_string()),
            Err(e) => ItemOutcome::Failed(describe(&e)),
        };
        push(items, STEP, "follow", followed);
    }

    /// Both secrets, and the signing key inside the env, before the server is touched.
    async fn read_secrets(&self) -> Result<(SecretValue, SecretValue), DomainError> {
        let ghcr = self.secrets.get(GHCR_SECRET).await?;
        let env = self.secrets.get(CDN_SECRET).await?;
        if !env_has_value(env.expose(), SIGNING_KEY) {
            return Err(DomainError::ValidationError(format!(
                "{SIGNING_KEY} missing from {CDN_SECRET}: refusing to build"
            )));
        }
        Ok((ghcr, env))
    }
}
