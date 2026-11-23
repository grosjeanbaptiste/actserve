use std::sync::Arc;

use crate::application::fleet_status_service::{FleetStatusService, HostReport, render};
use crate::domain::{
    bootstrap::{Declared, plan},
    errors::DomainError,
    fleet::HostStatus,
};
use crate::ports::outbound::{
    declared_data::DeclaredDataPort, hostname_probe::HostnameProbePort,
    remote_shell::RemoteShellPort, server_inventory::ServerInventoryPort,
};

/// What a run printed, line by line, and whether the server already conforms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyReport {
    pub name: String,
    pub lines: Vec<String>,
    pub conforming: bool,
}

impl ApplyReport {
    /// 0 when nothing would change, 1 otherwise.
    pub fn exit_code(&self) -> u8 {
        u8::from(!self.conforming)
    }
}

pub struct FleetApplyService {
    status: FleetStatusService,
    inventory: Arc<dyn ServerInventoryPort>,
    shell: Arc<dyn RemoteShellPort>,
    data: Arc<dyn DeclaredDataPort>,
}

impl FleetApplyService {
    pub fn new(
        inventory: Arc<dyn ServerInventoryPort>,
        probe: Arc<dyn HostnameProbePort>,
        shell: Arc<dyn RemoteShellPort>,
        data: Arc<dyn DeclaredDataPort>,
    ) -> Self {
        Self {
            status: FleetStatusService::new(inventory.clone(), probe),
            inventory,
            shell,
            data,
        }
    }

    async fn declared(&self, name: &str) -> Result<Declared, DomainError> {
        Ok(Declared {
            sshd_conf: self.data.read(name, "sshd.conf").await?,
            ports: self.data.read(name, "ports.txt").await?,
            docker_daemon: self.data.read(name, "docker-daemon.json").await?,
            dirs: self.data.read(name, "dirs.txt").await?,
            stacks: self.data.read(name, "stacks.txt").await?,
        })
    }

    /// Check every item of the bootstrap on one server and say what would change.
    /// Sends read-only commands only.
    pub async fn dry_run(&self, name: &str) -> Result<ApplyReport, DomainError> {
        self.run(name, false).await
    }

    /// Check every item and, for each one that drifted, run its fix then check again.
    pub async fn apply(&self, name: &str) -> Result<ApplyReport, DomainError> {
        self.run(name, true).await
    }

    async fn run(&self, name: &str, write: bool) -> Result<ApplyReport, DomainError> {
        let report = |lines: Vec<String>, conforming| ApplyReport {
            name: name.to_string(),
            lines,
            conforming,
        };
        let checked = self.status.status(Some(name)).await?;
        let status = checked.into_iter().next().map(|r| r.status);
        if let Some(bad) = status.filter(|s| *s != HostStatus::Ok) {
            let line = render(&[HostReport {
                name: name.to_string(),
                status: bad,
            }]);
            return Ok(report(vec![line.trim_end().to_string()], false));
        }
        let hosts = self.inventory.hosts().await?;
        let target = hosts
            .iter()
            .find(|h| h.name() == name)
            .map(|h| h.ssh_target().to_string())
            .ok_or_else(|| DomainError::NotFound {
                entity: "FleetHost",
                id: name.to_string(),
            })?;

        let reach = self
            .shell
            .run(&target, "test \"$(id -u)\" -ne 0 && sudo -n true", &[])
            .await;
        if !reach.is_ok_and(|o| o.success()) {
            let why = "fail (needs a non-root user with passwordless sudo and the operator key)";
            return Ok(report(vec![format!("{:<10} {why}", "reach")], false));
        }
        let mut lines = vec![format!("{:<10} ok", "reach")];

        let items = match self.declared(name).await.and_then(|d| plan(&d)) {
            Ok(items) => items,
            Err(e) => {
                lines.push(format!("declared   fail ({e})"));
                return Ok(report(lines, false));
            }
        };
        let mut conforming = true;
        for item in &items {
            let label = format!("{} {:<9}", item.step, item.name);
            if item.held {
                lines.push(format!("{label} skipped (held back)"));
                continue;
            }
            match self
                .shell
                .run(&target, &item.check.command, &item.check.stdin)
                .await
            {
                Ok(o) if o.success() => lines.push(format!("{label} ok")),
                Ok(_) => match item.fix.as_ref().filter(|_| write) {
                    Some(fix) => {
                        let fixed = self
                            .shell
                            .run(&target, &fix.command, &fix.stdin)
                            .await
                            .is_ok_and(|o| o.success());
                        if !fixed {
                            conforming = false;
                            lines.push(format!("{label} fail (fix failed)"));
                            continue;
                        }
                        let healed = self
                            .shell
                            .run(&target, &item.check.command, &item.check.stdin)
                            .await
                            .is_ok_and(|o| o.success());
                        if healed {
                            lines.push(format!("{label} applied"));
                        } else {
                            conforming = false;
                            lines.push(format!("{label} fail (still drifted after apply)"));
                        }
                    }
                    None => {
                        conforming = false;
                        lines.push(format!("{label} would apply"));
                    }
                },
                Err(e) => {
                    lines.push(format!("{label} fail ({e})"));
                    return Ok(report(lines, false));
                }
            }
        }
        Ok(report(lines, conforming))
    }
}
