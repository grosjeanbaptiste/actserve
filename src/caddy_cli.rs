//! `actserve deploy-caddy NAME [--dry-run]`: only the Caddyfile of `origin/main`
//! of `$ACTEBLE_API_DIR`, then a Caddy reload. Git is `$ACTSERVE_GIT` (default `git`).
use std::process::ExitCode;
use std::sync::Arc;

use crate::adapters::outbound::system::{
    actesskey_secret_store::ActesskeySecretStore, git_caddyfile_source::GitCaddyfileSource,
    ssh_hostname_probe::SshHostnameProbe, ssh_remote_shell::SshRemoteShell,
};
use crate::application::caddy_deploy_service::CaddyDeployService;
use crate::deploy_cli::{CliResult, api_dir, finish, split_flag};
use crate::fleet_cli::inventory_and_ssh;

const USAGE: &str = "usage: actserve deploy-caddy NAME [--dry-run]";

fn service() -> Result<CaddyDeployService, Box<dyn std::error::Error>> {
    let (inventory, ssh) = inventory_and_ssh()?;
    let git = std::env::var("ACTSERVE_GIT").unwrap_or_else(|_| "git".to_string());
    let key = std::env::var("ACTSERVE_ACTESSKEY").unwrap_or_else(|_| "actesskey".to_string());
    Ok(CaddyDeployService::new(
        inventory,
        Arc::new(SshHostnameProbe::new(ssh.clone())),
        Arc::new(SshRemoteShell::new(ssh)),
        Arc::new(GitCaddyfileSource::new(git)),
        Arc::new(ActesskeySecretStore::new(key)),
    ))
}

pub async fn run_deploy_caddy(args: &[String]) -> CliResult {
    let parsed = split_flag(args).and_then(|(rest, dry)| match rest.as_slice() {
        [name] => Some((*name, dry)),
        _ => None,
    });
    let Some((name, dry_run)) = parsed else {
        eprintln!("{USAGE}");
        return Ok(ExitCode::from(2));
    };
    finish(
        name,
        service()?.deploy_caddy(name, &api_dir()?, dry_run).await,
    )
}
