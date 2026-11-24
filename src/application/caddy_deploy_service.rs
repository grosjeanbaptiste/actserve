use std::path::Path;
use std::sync::Arc;

use crate::application::{
    fleet_status_service::probe_status,
    harden_service::HardenRun,
    item_runner::{failed, find_host, push},
    remote_env::with_remote_env_value,
};
use crate::domain::{
    caddy::{backup_command, reload_script, write_command},
    deploy::{API_ENV_PATH, API_SECRET},
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    harden::{ItemOutcome, ItemReport},
};
use crate::ports::outbound::{
    caddyfile_source::CaddyfileSourcePort,
    hostname_probe::HostnameProbePort,
    remote_shell::{RemoteOutput, RemoteShellPort},
    secret_store::SecretStorePort,
    server_inventory::ServerInventoryPort,
};

const STEP: &str = "deploy-caddy";

/// Pushes only the Caddyfile of `origin/main` and reloads Caddy, without
/// touching the API image. Caddy validates the config on reload; a refusal
/// restores the backup. Same contract as `DeployService`.
pub struct CaddyDeployService {
    inventory: Arc<dyn ServerInventoryPort>,
    probe: Arc<dyn HostnameProbePort>,
    shell: Arc<dyn RemoteShellPort>,
    source: Arc<dyn CaddyfileSourcePort>,
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

fn outcome(ran: Result<RemoteOutput, DomainError>) -> ItemOutcome {
    match ran {
        Ok(out) if out.code == Some(0) => ItemOutcome::Applied,
        Ok(out) => ItemOutcome::Failed(failed(&out)),
        Err(e) => ItemOutcome::Failed(describe(&e)),
    }
}

impl CaddyDeployService {
    pub fn new(
        inventory: Arc<dyn ServerInventoryPort>,
        probe: Arc<dyn HostnameProbePort>,
        shell: Arc<dyn RemoteShellPort>,
        source: Arc<dyn CaddyfileSourcePort>,
        secrets: Arc<dyn SecretStorePort>,
    ) -> Self {
        Self {
            inventory,
            probe,
            shell,
            source,
            secrets,
        }
    }

    pub async fn deploy_caddy(
        &self,
        name: &str,
        api_dir: &Path,
        dry_run: bool,
    ) -> Result<HardenRun, DomainError> {
        let host = find_host(&*self.inventory, name).await?;
        let status = probe_status(&*self.probe, &host).await;
        if status != HostStatus::Ok {
            return Ok(HardenRun::Refused(status));
        }
        let mut items = Vec::new();
        if dry_run {
            for item in ["source", "backup", "copy", "reload"] {
                push(&mut items, STEP, item, ItemOutcome::WouldApply);
            }
            return Ok(HardenRun::Done(items));
        }
        self.run(&mut items, &host, api_dir).await;
        Ok(HardenRun::Done(items))
    }

    /// Source, secret, backup, write, reload; stops at the first failure.
    async fn run(&self, items: &mut Vec<ItemReport>, host: &FleetHost, api_dir: &Path) {
        let caddyfile = match self.source.origin_main(api_dir).await {
            Ok(text) => text,
            Err(e) => {
                push(items, STEP, "source", ItemOutcome::Failed(describe(&e)));
                return;
            }
        };
        push(items, STEP, "source", ItemOutcome::Applied);
        let env = match self.secrets.get(API_SECRET).await {
            Ok(env) => env,
            Err(e) => {
                push(items, STEP, "secrets", ItemOutcome::Failed(describe(&e)));
                return;
            }
        };
        let target = host.ssh_target();
        let backup = self.shell.run(target, &backup_command(), None).await;
        if !push(items, STEP, "backup", outcome(backup)) {
            return;
        }
        let write = self
            .shell
            .run(target, &write_command(), Some(&caddyfile))
            .await;
        if !push(items, STEP, "copy", outcome(write)) {
            return;
        }
        let reload = with_remote_env_value(
            &*self.shell,
            target,
            &env,
            API_ENV_PATH,
            &reload_script(),
            None,
        )
        .await;
        push(items, STEP, "reload", outcome(reload));
    }
}
