//! The server commands of the binary: `actserve status [NAME]` and
//! `actserve audit NAME`. The inventory is `$DOTFILES_STATE_ROOT/servers.txt`
//! (default `~/machine`), ssh is `$DOTFILES_SSH` (default `ssh`).
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use crate::adapters::outbound::system::{
    file_inventory::FileServerInventory, ssh_edge_audit::SshEdgeAudit,
    ssh_hostname_probe::SshHostnameProbe,
};
use crate::application::{
    fleet_audit_service::{self, FleetAuditService},
    fleet_status_service::{FleetStatusService, all_ok, render},
};
use crate::domain::errors::DomainError;

type CliResult = Result<ExitCode, Box<dyn std::error::Error>>;

fn inventory_and_ssh() -> Result<(Arc<FileServerInventory>, PathBuf), Box<dyn std::error::Error>> {
    let root = std::env::var("DOTFILES_STATE_ROOT")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join("machine")))?;
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
