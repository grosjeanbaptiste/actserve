//! `actserve edge-backup NAME [DIR]`: DIR defaults to `$HOME/Backups/acteble-edge`.
//! Exit 0 saved, 1 failed, 3 not the expected host, 2 unknown name or usage.
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use crate::adapters::outbound::system::{
    ssh_hostname_probe::SshHostnameProbe, ssh_remote_download::SshRemoteDownload,
};
use crate::application::edge_backup_service::{EdgeBackupService, exit_code, render};
use crate::deploy_cli::CliResult;
use crate::domain::{backup::DEFAULT_DIR, errors::DomainError};
use crate::fleet_cli::inventory_and_ssh;

const USAGE: &str = "usage: actserve edge-backup NAME [DIR]";

fn service() -> Result<EdgeBackupService, Box<dyn std::error::Error>> {
    let (inventory, ssh) = inventory_and_ssh()?;
    Ok(EdgeBackupService::new(
        inventory,
        Arc::new(SshHostnameProbe::new(ssh.clone())),
        Arc::new(SshRemoteDownload::new(ssh)),
    ))
}

fn directory(given: Option<&String>) -> Result<PathBuf, Box<dyn std::error::Error>> {
    match given {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => Ok(PathBuf::from(std::env::var("HOME")?).join(DEFAULT_DIR)),
    }
}

pub async fn run_edge_backup(args: &[String]) -> CliResult {
    let [name, rest @ ..] = args else {
        eprintln!("{USAGE}");
        return Ok(ExitCode::from(2));
    };
    if name.starts_with('-') || rest.len() > 1 || rest.iter().any(|a| a.starts_with('-')) {
        eprintln!("{USAGE}");
        return Ok(ExitCode::from(2));
    }
    let stamp = chrono::Local::now().format("%Y%m%dT%H%M%S").to_string();
    match service()?
        .backup(name, &directory(rest.first())?, &stamp)
        .await
    {
        Ok(run) => {
            let text = render(name, &run);
            if exit_code(&run) == 1 {
                eprint!("{text}");
            } else {
                print!("{text}");
            }
            Ok(ExitCode::from(exit_code(&run)))
        }
        Err(DomainError::NotFound { id, .. }) => {
            eprintln!("actserve: unknown server: {id}");
            Ok(ExitCode::from(2))
        }
        Err(e) => Err(e.into()),
    }
}
