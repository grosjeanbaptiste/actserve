use std::sync::Arc;

use crate::application::{
    fleet_status_service::probe_status,
    item_runner::{Cmd, check_apply, find_host, push},
};
use crate::domain::{
    baseline,
    errors::DomainError,
    fleet::HostStatus,
    harden::{ItemOutcome, ItemReport, SshdOutcome},
    ports_file::parse_ports,
};
use crate::ports::outbound::{
    declared_data::DeclaredDataPort, hostname_probe::HostnameProbePort,
    remote_shell::RemoteShellPort, server_inventory::ServerInventoryPort,
    sshd_installer::SshdInstallerPort,
};

/// Not run at all (`Refused`: wrong or unreachable host) or run to its end or
/// first failure (`Done`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardenRun {
    Refused(HostStatus),
    Done(Vec<ItemReport>),
}

const REACH_CHECK: &str = r#"test "$(id -u)" != 0 && sudo -n true"#;

pub struct HardenService {
    inventory: Arc<dyn ServerInventoryPort>,
    probe: Arc<dyn HostnameProbePort>,
    data: Arc<dyn DeclaredDataPort>,
    shell: Arc<dyn RemoteShellPort>,
    sshd: Arc<dyn SshdInstallerPort>,
}

impl HardenService {
    pub fn new(
        inventory: Arc<dyn ServerInventoryPort>,
        probe: Arc<dyn HostnameProbePort>,
        data: Arc<dyn DeclaredDataPort>,
        shell: Arc<dyn RemoteShellPort>,
        sshd: Arc<dyn SshdInstallerPort>,
    ) -> Self {
        Self {
            inventory,
            probe,
            data,
            shell,
            sshd,
        }
    }

    /// Harden `name`. Declared data is validated before any remote call; the
    /// run stops at the first failed step. With `dry_run` nothing is applied.
    pub async fn run(&self, name: &str, dry_run: bool) -> Result<HardenRun, DomainError> {
        let host = find_host(&*self.inventory, name).await?;
        let conf = self.data.read(name, "sshd.conf").await?;
        let ports = parse_ports("ports.txt", &self.data.read(name, "ports.txt").await?)?;
        let status = probe_status(&*self.probe, &host).await;
        if status != HostStatus::Ok {
            return Ok(HardenRun::Refused(status));
        }
        let t = host.ssh_target();
        let mut items = Vec::new();
        let reach = self.shell.run(t, REACH_CHECK, None).await?;
        let reach = if reach.code == Some(0) {
            ItemOutcome::Ok
        } else {
            ItemOutcome::Failed("needs a non-root user with passwordless sudo".to_string())
        };
        let mut ok = push(&mut items, "reach", "reach", reach);
        if ok {
            let o = self.sshd_item(t, &conf, dry_run).await?;
            ok = push(&mut items, "harden", "sshd", o);
        }
        let ufw = (
            baseline::ufw_check_command(&ports),
            baseline::ufw_apply_command(&ports),
        );
        let rest = [
            ("ufw", ufw.0.as_str(), ufw.1.as_str()),
            (
                "fail2ban",
                baseline::FAIL2BAN_CHECK,
                baseline::FAIL2BAN_APPLY,
            ),
            (
                "upgrades",
                baseline::UPGRADES_CHECK,
                baseline::UPGRADES_APPLY,
            ),
        ];
        for (item, check, apply) in rest {
            if !ok {
                break;
            }
            let o =
                check_apply(&*self.shell, t, Cmd(check, None), Cmd(apply, None), dry_run).await?;
            ok = push(&mut items, "harden", item, o);
        }
        Ok(HardenRun::Done(items))
    }

    async fn sshd_item(
        &self,
        target: &str,
        conf: &str,
        dry_run: bool,
    ) -> Result<ItemOutcome, DomainError> {
        let check = baseline::sshd_check_command();
        let out = self.shell.run(target, &check, Some(conf)).await?;
        if out.code == Some(0) {
            return Ok(ItemOutcome::Ok);
        }
        if dry_run {
            return Ok(ItemOutcome::WouldApply);
        }
        Ok(match self.sshd.install(target, conf).await? {
            SshdOutcome::Installed => ItemOutcome::Applied,
            SshdOutcome::RolledBack => ItemOutcome::Failed(
                "fresh connection failed after the change, rolled back".to_string(),
            ),
            SshdOutcome::Failed { reason } => ItemOutcome::Failed(reason),
        })
    }
}

/// 0 when the run was not refused and no step failed, 1 when a step failed,
/// 3 when the host was refused.
pub fn exit_code(run: &HardenRun) -> u8 {
    match run {
        HardenRun::Refused(_) => 3,
        HardenRun::Done(items) => u8::from(
            items
                .iter()
                .any(|i| matches!(i.outcome, ItemOutcome::Failed(_))),
        ),
    }
}

pub fn render(name: &str, run: &HardenRun) -> String {
    match run {
        HardenRun::Refused(HostStatus::WrongHost { got, expected }) => {
            format!("{name}: wrong-host (answered {got}, expected {expected})\n")
        }
        HardenRun::Refused(HostStatus::Unreachable { reason }) => {
            format!("{name}: unreachable ({reason})\n")
        }
        HardenRun::Refused(HostStatus::Ok) => String::new(),
        HardenRun::Done(items) => items.iter().map(line).collect(),
    }
}

fn line(r: &ItemReport) -> String {
    let label = if r.step == r.item {
        format!("{:<11}", r.item)
    } else {
        format!("{} {:<9} ", r.step, r.item)
    };
    let status = match &r.outcome {
        ItemOutcome::Ok => "ok".to_string(),
        ItemOutcome::Applied => "applied".to_string(),
        ItemOutcome::WouldApply => "would apply".to_string(),
        ItemOutcome::Failed(m) => format!("fail ({m})"),
    };
    let note = match (r.item, &r.outcome) {
        ("logs", ItemOutcome::Applied) => format!(
            "{} logs: installed; it takes effect at the next restart of Docker, containers keep running\n",
            r.step
        ),
        _ => String::new(),
    };
    format!("{label}{status}\n{note}")
}
