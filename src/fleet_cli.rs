//! The server commands of the binary: `actserve status [NAME]` and
//! `actserve audit NAME` and
//! `actserve harden NAME [--dry-run]`. The inventory is `$DOTFILES_STATE_ROOT/servers.txt`
//! (default `~/machine`), ssh is `$DOTFILES_SSH` (default `ssh`).
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use crate::adapters::outbound::system::{
    file_declared_data::FileDeclaredData, file_inventory::FileServerInventory,
    ssh_edge_audit::SshEdgeAudit, ssh_hostname_probe::SshHostnameProbe,
    ssh_remote_shell::SshRemoteShell, ssh_sshd_installer::SshSshdInstaller,
};
use crate::application::{
    bootstrap_service::BootstrapService,
    fleet_audit_service::{self, FleetAuditService},
    fleet_status_service::{FleetStatusService, all_ok, render},
    harden_service::{self, HardenRun, HardenService},
    runtime_service::RuntimeService,
    stacks_service::StacksService,
};
use crate::domain::errors::DomainError;
use crate::stack_wiring::stack_deploy;

type CliResult = Result<ExitCode, Box<dyn std::error::Error>>;

fn state_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(std::env::var("DOTFILES_STATE_ROOT")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join("machine")))?)
}

pub(crate) fn inventory_and_ssh()
-> Result<(Arc<FileServerInventory>, PathBuf), Box<dyn std::error::Error>> {
    let root = state_root()?;
    let ssh = std::env::var("DOTFILES_SSH").unwrap_or_else(|_| "ssh".to_string());
    Ok((
        Arc::new(FileServerInventory::new(root.join("servers.txt"))),
        PathBuf::from(ssh),
    ))
}

/// Ask each server for its hostname. Exit 0 only when all answer as expected.
pub async fn run_status(name: Option<&str>) -> CliResult {
    let (inventory, ssh) = inventory_and_ssh()?;
    let service = FleetStatusService::new(inventory, Arc::new(SshHostnameProbe::new(ssh)));
    let reports = service.status(name).await?;
    print!("{}", render(&reports));
    Ok(if all_ok(&reports) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Audit one server. Exit 0 conforming, 1 deviation or not the expected
/// host, 3 unreadable, 2 unknown name or no name.
pub async fn run_audit(name: Option<&str>) -> CliResult {
    let Some(name) = name else {
        eprintln!("usage: actserve audit NAME");
        return Ok(ExitCode::from(2));
    };
    let (inventory, ssh) = inventory_and_ssh()?;
    let service = FleetAuditService::new(
        inventory,
        Arc::new(SshHostnameProbe::new(ssh.clone())),
        Arc::new(SshEdgeAudit::new(ssh)),
    );
    match service.audit(name).await {
        Ok(run) => {
            print!("{}", fleet_audit_service::render(name, &run));
            Ok(ExitCode::from(fleet_audit_service::exit_code(&run)))
        }
        Err(DomainError::NotFound { id, .. }) => {
            eprintln!("actserve: unknown server: {id}");
            Ok(ExitCode::from(2))
        }
        Err(e) => Err(e.into()),
    }
}

const HARDEN_USAGE: &str = "usage: actserve harden NAME [--dry-run]";
const EDGE_USAGE: &str = "usage: actserve edge NAME [--dry-run]";
const STACKS_USAGE: &str = "usage: actserve stacks NAME [--dry-run]";
const RUNTIME_USAGE: &str = "usage: actserve runtime NAME [--dry-run]";

/// `NAME [--dry-run]`; anything else is a usage error.
fn name_and_flag(args: &[String]) -> Option<(&str, bool)> {
    match args {
        [n] if !n.starts_with('-') => Some((n.as_str(), false)),
        [n, f] if !n.starts_with('-') && f == "--dry-run" => Some((n.as_str(), true)),
        _ => None,
    }
}

/// Prints a step's run and maps it to its exit code: 0 done (or dry run),
/// 1 a step failed or the declared data is invalid, 3 not the expected host,
/// 2 unknown name.
fn finish(name: &str, run: Result<HardenRun, DomainError>) -> CliResult {
    match run {
        Ok(run) => {
            print!("{}", harden_service::render(name, &run));
            Ok(ExitCode::from(harden_service::exit_code(&run)))
        }
        Err(DomainError::NotFound { id, .. }) => {
            eprintln!("actserve: unknown server: {id}");
            Ok(ExitCode::from(2))
        }
        Err(DomainError::ValidationError(m)) => {
            eprintln!("actserve: {m}");
            Ok(ExitCode::from(1))
        }
        Err(e) => Err(e.into()),
    }
}

/// Harden one server (`actserve harden NAME [--dry-run]`).
pub async fn run_harden(args: &[String]) -> CliResult {
    let Some((name, dry_run)) = name_and_flag(args) else {
        eprintln!("{HARDEN_USAGE}");
        return Ok(ExitCode::from(2));
    };
    let (inventory, ssh) = inventory_and_ssh()?;
    let service = HardenService::new(
        inventory,
        Arc::new(SshHostnameProbe::new(ssh.clone())),
        Arc::new(FileDeclaredData::new(state_root()?)),
        Arc::new(SshRemoteShell::new(ssh.clone())),
        Arc::new(SshSshdInstaller::new(ssh)),
    );
    finish(name, service.run(name, dry_run).await)
}

/// Docker, logs and stack directories of one server (`actserve runtime NAME [--dry-run]`).
pub async fn run_runtime(args: &[String]) -> CliResult {
    let Some((name, dry_run)) = name_and_flag(args) else {
        eprintln!("{RUNTIME_USAGE}");
        return Ok(ExitCode::from(2));
    };
    let (inventory, ssh) = inventory_and_ssh()?;
    let service = RuntimeService::new(
        inventory,
        Arc::new(SshHostnameProbe::new(ssh.clone())),
        Arc::new(FileDeclaredData::new(state_root()?)),
        Arc::new(SshRemoteShell::new(ssh)),
    );
    finish(name, service.run(name, dry_run).await)
}

fn stacks_service(
    ssh: PathBuf,
    inventory: Arc<FileServerInventory>,
) -> Result<StacksService, Box<dyn std::error::Error>> {
    Ok(StacksService::new(
        inventory,
        Arc::new(SshHostnameProbe::new(ssh.clone())),
        Arc::new(FileDeclaredData::new(state_root()?)),
        Arc::new(SshRemoteShell::new(ssh)),
        stack_deploy()?,
    ))
}

/// The edge of one server (`actserve edge NAME [--dry-run]`).
pub async fn run_edge(args: &[String]) -> CliResult {
    let Some((name, dry_run)) = name_and_flag(args) else {
        eprintln!("{EDGE_USAGE}");
        return Ok(ExitCode::from(2));
    };
    let (inventory, ssh) = inventory_and_ssh()?;
    finish(
        name,
        stacks_service(ssh, inventory)?.edge(name, dry_run).await,
    )
}

/// The declared stacks of one server (`actserve stacks NAME [--dry-run]`).
pub async fn run_stacks(args: &[String]) -> CliResult {
    let Some((name, dry_run)) = name_and_flag(args) else {
        eprintln!("{STACKS_USAGE}");
        return Ok(ExitCode::from(2));
    };
    let (inventory, ssh) = inventory_and_ssh()?;
    finish(
        name,
        stacks_service(ssh, inventory)?.stacks(name, dry_run).await,
    )
}

/// `actserve bootstrap NAME [--dry-run]`, and `apply` (the same run on a
/// server that exists): reach, harden, runtime, edge, stacks, audit.
pub async fn run_bootstrap(verb: &str, args: &[String]) -> CliResult {
    let Some((name, dry_run)) = name_and_flag(args) else {
        eprintln!("usage: actserve {verb} NAME [--dry-run]");
        return Ok(ExitCode::from(2));
    };
    let (inventory, ssh) = inventory_and_ssh()?;
    let probe = Arc::new(SshHostnameProbe::new(ssh.clone()));
    let data = Arc::new(FileDeclaredData::new(state_root()?));
    let shell = Arc::new(SshRemoteShell::new(ssh.clone()));
    let service = BootstrapService::new(
        inventory.clone(),
        data.clone(),
        Arc::new(HardenService::new(
            inventory.clone(),
            probe.clone(),
            data.clone(),
            shell.clone(),
            Arc::new(SshSshdInstaller::new(ssh.clone())),
        )),
        Arc::new(RuntimeService::new(
            inventory.clone(),
            probe.clone(),
            data.clone(),
            shell.clone(),
        )),
        Arc::new(StacksService::new(
            inventory.clone(),
            probe.clone(),
            data,
            shell,
            stack_deploy()?,
        )),
        Arc::new(FleetAuditService::new(
            inventory,
            probe,
            Arc::new(SshEdgeAudit::new(ssh)),
        )),
    );
    finish(name, service.run(name, dry_run).await)
}
