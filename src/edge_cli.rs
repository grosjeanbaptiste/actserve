//! `actserve edge-deploy NAME [--dry-run]`. The files come from `$ACTEBLE_EDGE_DIR`
//! (default `~/Source/Acteble/acteble_edge`); scp is `$DOTFILES_SCP`, secrets `$ACTSERVE_ACTESSKEY`.
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use crate::adapters::outbound::system::{
    actesskey_secret_store::ActesskeySecretStore, scp_remote_copy::ScpRemoteCopy,
    ssh_hostname_probe::SshHostnameProbe, ssh_remote_shell::SshRemoteShell,
};
use crate::application::edge_deploy_service::EdgeDeployService;
use crate::deploy_cli::{CliResult, finish, split_flag};
use crate::fleet_cli::inventory_and_ssh;

const USAGE: &str = "usage: actserve edge-deploy NAME [--dry-run]";

fn service() -> Result<EdgeDeployService, Box<dyn std::error::Error>> {
    let (inventory, ssh) = inventory_and_ssh()?;
    let scp = std::env::var("DOTFILES_SCP").unwrap_or_else(|_| "scp".to_string());
    let key = std::env::var("ACTSERVE_ACTESSKEY").unwrap_or_else(|_| "actesskey".to_string());
    Ok(EdgeDeployService::new(
        inventory,
        Arc::new(SshHostnameProbe::new(ssh.clone())),
        Arc::new(SshRemoteShell::new(ssh)),
        Arc::new(ScpRemoteCopy::new(scp)),
        Arc::new(ActesskeySecretStore::new(key)),
    ))
}

fn edge_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(std::env::var("ACTEBLE_EDGE_DIR")
        .map(PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME").map(|h| PathBuf::from(h).join("Source/Acteble/acteble_edge"))
        })?)
}

pub async fn run_edge_deploy(args: &[String]) -> CliResult {
    let parsed = split_flag(args).and_then(|(rest, dry)| match rest.as_slice() {
        [name] => Some((*name, dry)),
        _ => None,
    });
    let Some((name, dry_run)) = parsed else {
        eprintln!("{USAGE}");
        return Ok(ExitCode::from(2));
    };
    finish(name, service()?.deploy(name, &edge_dir()?, dry_run).await)
}
