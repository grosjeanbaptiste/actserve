//! `actserve cdn-build-geo NAME [--stage catalog|tree|publish|all] [--dry-run]`.
//! Secrets come from `$ACTSERVE_ACTESSKEY`, ssh from `$DOTFILES_SSH`.
use std::process::ExitCode;
use std::sync::Arc;

use crate::adapters::outbound::system::{
    actesskey_secret_store::ActesskeySecretStore, ssh_hostname_probe::SshHostnameProbe,
    ssh_remote_shell::SshRemoteShell, ssh_terminal::SshTerminal,
};
use crate::application::geo_build_service::GeoBuildService;
use crate::deploy_cli::{CliResult, finish};
use crate::domain::geo_build::GeoStage;
use crate::fleet_cli::inventory_and_ssh;

const USAGE: &str =
    "usage: actserve cdn-build-geo NAME [--stage catalog|tree|publish|all] [--dry-run]";

struct Request<'a> {
    name: &'a str,
    stage: &'a str,
    dry_run: bool,
}

/// One name, optionally `--stage S` and `--dry-run`; anything else is a usage error.
fn parse(args: &[String]) -> Option<Request<'_>> {
    let (mut name, mut stage, mut dry_run) = (None, "all", false);
    let mut it = args.iter().map(String::as_str);
    while let Some(arg) = it.next() {
        match arg {
            "--dry-run" => dry_run = true,
            "--stage" => stage = it.next().filter(|s| !s.starts_with('-'))?,
            a if a.starts_with('-') => return None,
            a if name.replace(a).is_none() => {}
            _ => return None,
        }
    }
    Some(Request {
        name: name?,
        stage,
        dry_run,
    })
}

fn service() -> Result<GeoBuildService, Box<dyn std::error::Error>> {
    let (inventory, ssh) = inventory_and_ssh()?;
    let key = std::env::var("ACTSERVE_ACTESSKEY").unwrap_or_else(|_| "actesskey".to_string());
    Ok(GeoBuildService::new(
        inventory,
        Arc::new(SshHostnameProbe::new(ssh.clone())),
        Arc::new(SshRemoteShell::new(ssh.clone())),
        Arc::new(SshTerminal::new(ssh)),
        Arc::new(ActesskeySecretStore::new(key)),
    ))
}

pub async fn run_cdn_build_geo(args: &[String]) -> CliResult {
    let Some(req) = parse(args) else {
        eprintln!("{USAGE}");
        return Ok(ExitCode::from(2));
    };
    // A bad stage is refused before any server or secret is looked at.
    let stage = match GeoStage::parse(req.stage) {
        Ok(s) => s,
        Err(e) => return finish(req.name, Err(e)),
    };
    finish(
        req.name,
        service()?.build(req.name, stage, req.dry_run).await,
    )
}
