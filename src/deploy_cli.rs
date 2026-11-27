//! `actserve deploy NAME [--dry-run]` and `actserve rollback NAME [TAG] [--dry-run]`.
//! The API files come from `$ACTEBLE_API_DIR` (default `~/Source/Acteble/acteble_api`),
//! scp is `$ACTSERVE_SCP` (default `scp`), secrets come from `$ACTSERVE_ACTESSKEY`
//! (default `actesskey`).
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use crate::adapters::outbound::system::{
    actesskey_secret_store::ActesskeySecretStore, scp_remote_copy::ScpRemoteCopy,
    ssh_hostname_probe::SshHostnameProbe, ssh_remote_shell::SshRemoteShell,
};
use crate::application::{
    deploy_service::DeployService,
    harden_service::{self, HardenRun},
};
use crate::domain::errors::DomainError;
use crate::fleet_cli::inventory_and_ssh;

pub(crate) type CliResult = Result<ExitCode, Box<dyn std::error::Error>>;

const DEPLOY_USAGE: &str = "usage: actserve deploy NAME [--dry-run]";
const ROLLBACK_USAGE: &str = "usage: actserve rollback NAME [TAG] [--dry-run]";

pub(crate) fn service() -> Result<DeployService, Box<dyn std::error::Error>> {
    let (inventory, ssh) = inventory_and_ssh()?;
    let scp = crate::fleet_cli::program("ACTSERVE_SCP", "scp");
    let key = std::env::var("ACTSERVE_ACTESSKEY").unwrap_or_else(|_| "actesskey".to_string());
    Ok(DeployService::new(
        inventory,
        Arc::new(SshHostnameProbe::new(ssh.clone())),
        Arc::new(SshRemoteShell::new(ssh)),
        Arc::new(ScpRemoteCopy::new(scp)),
        Arc::new(ActesskeySecretStore::new(key)),
    ))
}

pub(crate) fn api_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(std::env::var("ACTEBLE_API_DIR")
        .map(PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME").map(|h| PathBuf::from(h).join("Source/Acteble/acteble_api"))
        })?)
}

/// Exit 0 done, 1 a step failed or invalid input, 3 not the expected host, 2 unknown name.
pub(crate) fn finish(name: &str, run: Result<HardenRun, DomainError>) -> CliResult {
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

/// Splits `--dry-run` from the other arguments; a lone dash-word is not a name or tag.
pub(crate) fn split_flag(args: &[String]) -> Option<(Vec<&str>, bool)> {
    let dry = args.iter().any(|a| a == "--dry-run");
    let rest: Vec<&str> = args
        .iter()
        .filter(|a| *a != "--dry-run")
        .map(String::as_str)
        .collect();
    rest.iter()
        .all(|a| !a.starts_with('-'))
        .then_some((rest, dry))
}

pub async fn run_deploy(args: &[String]) -> CliResult {
    let Some((rest, dry_run)) = split_flag(args) else {
        eprintln!("{DEPLOY_USAGE}");
        return Ok(ExitCode::from(2));
    };
    let [name] = rest.as_slice() else {
        eprintln!("{DEPLOY_USAGE}");
        return Ok(ExitCode::from(2));
    };
    finish(name, service()?.deploy(name, &api_dir()?, dry_run).await)
}

pub async fn run_rollback(args: &[String]) -> CliResult {
    let parsed = split_flag(args).and_then(|(rest, dry)| match rest.as_slice() {
        [n] => Some((*n, "previous", dry)),
        [n, t] => Some((*n, *t, dry)),
        _ => None,
    });
    let Some((name, tag, dry_run)) = parsed else {
        eprintln!("{ROLLBACK_USAGE}");
        return Ok(ExitCode::from(2));
    };
    finish(name, service()?.rollback(name, tag, dry_run).await)
}
