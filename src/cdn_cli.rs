//! `actserve cdn-deploy NAME [--dry-run]`. The compose file comes from `$ACTEBLE_CDN_DIR`
//! (default `~/Source/Acteble/acteble_cdn`); scp is `$DOTFILES_SCP`, secrets `$ACTSERVE_ACTESSKEY`.
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use crate::adapters::outbound::system::{
    actesskey_secret_store::ActesskeySecretStore, scp_remote_copy::ScpRemoteCopy,
    ssh_hostname_probe::SshHostnameProbe, ssh_remote_shell::SshRemoteShell,
};
use crate::application::cdn_deploy_service::CdnDeployService;
use crate::deploy_cli::{CliResult, finish, split_flag};
use crate::fleet_cli::inventory_and_ssh;

const USAGE: &str = "usage: actserve cdn-deploy NAME [--dry-run]";

fn service() -> Result<CdnDeployService, Box<dyn std::error::Error>> {
    let (inventory, ssh) = inventory_and_ssh()?;
    let scp = std::env::var("DOTFILES_SCP").unwrap_or_else(|_| "scp".to_string());
    let key = std::env::var("ACTSERVE_ACTESSKEY").unwrap_or_else(|_| "actesskey".to_string());
    Ok(CdnDeployService::new(
        inventory,
        Arc::new(SshHostnameProbe::new(ssh.clone())),
        Arc::new(SshRemoteShell::new(ssh)),
        Arc::new(ScpRemoteCopy::new(scp)),
        Arc::new(ActesskeySecretStore::new(key)),
    ))
}

fn cdn_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(std::env::var("ACTEBLE_CDN_DIR")
        .map(PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME").map(|h| PathBuf::from(h).join("Source/Acteble/acteble_cdn"))
        })?)
}

pub async fn run_cdn_deploy(args: &[String]) -> CliResult {
    let parsed = split_flag(args).and_then(|(rest, dry)| match rest.as_slice() {
        [name] => Some((*name, dry)),
        _ => None,
    });
    let Some((name, dry_run)) = parsed else {
        eprintln!("{USAGE}");
        return Ok(ExitCode::from(2));
    };
    finish(name, service()?.deploy(name, &cdn_dir()?, dry_run).await)
}
