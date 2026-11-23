use std::sync::Arc;

use crate::application::{
    fleet_status_service::probe_status,
    harden_service::HardenRun,
    item_runner::{Cmd, check_apply, find_host, push},
};
use crate::domain::{
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    harden::{ItemOutcome, ItemReport},
    stacks::{self, DeployOutcome, StackMode, parse_stacks},
};
use crate::ports::outbound::{
    declared_data::DeclaredDataPort, hostname_probe::HostnameProbePort,
    remote_shell::RemoteShellPort, server_inventory::ServerInventoryPort,
    stack_deploy::StackDeployPort,
};

/// Steps 4 and 5 of the bootstrap: the edge (shared network, Caddy) and the
/// declared stacks. Same contract as `HardenService`; what a step starts, the
/// `StackDeployPort` starts.
pub struct StacksService {
    inventory: Arc<dyn ServerInventoryPort>,
    probe: Arc<dyn HostnameProbePort>,
    data: Arc<dyn DeclaredDataPort>,
    shell: Arc<dyn RemoteShellPort>,
    deploy: Arc<dyn StackDeployPort>,
}

impl StacksService {
    pub fn new(
        inventory: Arc<dyn ServerInventoryPort>,
        probe: Arc<dyn HostnameProbePort>,
        data: Arc<dyn DeclaredDataPort>,
        shell: Arc<dyn RemoteShellPort>,
        deploy: Arc<dyn StackDeployPort>,
    ) -> Self {
        Self {
            inventory,
            probe,
            data,
            shell,
            deploy,
        }
    }

    /// The host, once it answered as expected; otherwise the refusal.
    async fn host(&self, name: &str) -> Result<Result<FleetHost, HostStatus>, DomainError> {
        let host = find_host(&*self.inventory, name).await?;
        Ok(match probe_status(&*self.probe, &host).await {
            HostStatus::Ok => Ok(host),
            refused => Err(refused),
        })
    }

    /// Check; when it fails, start `stack` through the deploy port.
    async fn start_item(
        &self,
        target: &str,
        check: &str,
        stack: &str,
        dry_run: bool,
    ) -> Result<ItemOutcome, DomainError> {
        if self.shell.run(target, check, None).await?.code == Some(0) {
            return Ok(ItemOutcome::Ok);
        }
        if dry_run {
            return Ok(ItemOutcome::WouldApply);
        }
        Ok(match self.deploy.start(target, stack).await? {
            DeployOutcome::Started => ItemOutcome::Applied,
            DeployOutcome::Unavailable(reason) => ItemOutcome::Failed(reason),
        })
    }

    /// Step 4: the shared network, then the Caddy container.
    pub async fn edge(&self, name: &str, dry_run: bool) -> Result<HardenRun, DomainError> {
        let host = match self.host(name).await? {
            Ok(host) => host,
            Err(refused) => return Ok(HardenRun::Refused(refused)),
        };
        let t = host.ssh_target();
        let mut items = Vec::new();
        let net = check_apply(
            &*self.shell,
            t,
            Cmd(stacks::NETWORK_CHECK, None),
            Cmd(stacks::NETWORK_APPLY, None),
            dry_run,
        )
        .await?;
        if push(&mut items, "edge", "network", net) {
            let caddy = self
                .start_item(t, stacks::CADDY_CHECK, "edge", dry_run)
                .await?;
            push(&mut items, "edge", "caddy", caddy);
        }
        Ok(HardenRun::Done(items))
    }

    /// Step 5: the declared stacks in file order. A held stack is named and
    /// left alone; one to run is started only when it is not running.
    pub async fn stacks(&self, name: &str, dry_run: bool) -> Result<HardenRun, DomainError> {
        let find = find_host(&*self.inventory, name).await?;
        let declared = parse_stacks("stacks.txt", &self.data.read(name, "stacks.txt").await?)?;
        let host = match probe_status(&*self.probe, &find).await {
            HostStatus::Ok => find,
            refused => return Ok(HardenRun::Refused(refused)),
        };
        let mut items = Vec::new();
        for stack in declared {
            let item: &'static str = if stack.name == "cdn" { "cdn" } else { "api" };
            if stack.mode == StackMode::Held {
                items.push(ItemReport {
                    step: "stacks",
                    item,
                    outcome: ItemOutcome::Held,
                });
                continue;
            }
            let o = self
                .start_item(
                    host.ssh_target(),
                    stacks::cdn_check_command(),
                    item,
                    dry_run,
                )
                .await?;
            if !push(&mut items, "stacks", item, o) {
                break;
            }
        }
        Ok(HardenRun::Done(items))
    }
}
