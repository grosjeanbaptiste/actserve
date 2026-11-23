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
    fleet_audit_service::{self, FleetAuditService},
    fleet_status_service::{FleetStatusService, all_ok, render},
    harden_service::{self, HardenService},
};
use crate::domain::errors::DomainError;

type CliResult = Result<ExitCode, Box<dyn std::error::Error>>;

fn state_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(std::env::var("DOTFILES_STATE_ROOT")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join("machine")))?)
}

fn inventory_and_ssh() -> Result<(Arc<FileServerInventory>, PathBuf), Box<dyn std::error::Error>> {
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

/// Harden one server. Exit 0 done (or dry run), 1 a step failed or the
/// declared data is invalid, 3 not the expected host, 2 usage or unknown name.
pub async fn run_harden(args: &[String]) -> CliResult {
    let (name, dry_run) = match args {
        [n] if !n.starts_with('-') => (n.as_str(), false),
        [n, f] if !n.starts_with('-') && f == "--dry-run" => (n.as_str(), true),
        _ => {
            eprintln!("{HARDEN_USAGE}");
            return Ok(ExitCode::from(2));
        }
    };
    let (inventory, ssh) = inventory_and_ssh()?;
    let service = HardenService::new(
        inventory,
        Arc::new(SshHostnameProbe::new(ssh.clone())),
        Arc::new(FileDeclaredData::new(state_root()?)),
        Arc::new(SshRemoteShell::new(ssh.clone())),
        Arc::new(SshSshdInstaller::new(ssh)),
    );
    match service.run(name, dry_run).await {
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
