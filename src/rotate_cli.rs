//! `actserve rotate NAME TARGET [--dry-run] [--yes]`, TARGET one of jwt|postgres|minio|all.
//! A real rotation needs `--yes`. The database user is `$ACTEBLE_DB_USER` (default
//! `acteble_prod`), `machine` is `$ACTSERVE_MACHINE`, entropy is `/dev/urandom`; the API
//! files, scp and the secret store are those of `actserve deploy`.
//! Exit 0 done, 1 a step failed or invalid input, 3 not the expected host, 2 unknown name or usage.
use std::process::ExitCode;
use std::sync::Arc;

use crate::adapters::outbound::system::{
    actesskey_secret_store::ActesskeySecretStore, actesskey_secret_writer::ActesskeySecretWriter,
    machine_state_persist::MachineStatePersist, os_random::OsRandom,
    ssh_hostname_probe::SshHostnameProbe, ssh_remote_shell::SshRemoteShell,
};
use crate::application::rotate_service::{RotatePorts, RotateService};
use crate::deploy_cli::{CliResult, api_dir, finish};
use crate::domain::{
    errors::DomainError,
    rotate::{DEFAULT_DB_USER, RotateTarget},
};
use crate::fleet_cli::inventory_and_ssh;

const USAGE: &str = "usage: actserve rotate NAME jwt|postgres|minio|all [--dry-run] [--yes]";

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn service() -> Result<RotateService, Box<dyn std::error::Error>> {
    let (inventory, ssh) = inventory_and_ssh()?;
    let key = env_or("ACTSERVE_ACTESSKEY", "actesskey");
    let ports = RotatePorts {
        inventory,
        probe: Arc::new(SshHostnameProbe::new(ssh.clone())),
        shell: Arc::new(SshRemoteShell::new(ssh)),
        secrets: Arc::new(ActesskeySecretStore::new(&key)),
        writer: Arc::new(ActesskeySecretWriter::new(&key)),
        random: Arc::new(OsRandom::system()),
        persist: Arc::new(MachineStatePersist::new(env_or(
            "ACTSERVE_MACHINE",
            "machine",
        ))),
    };
    let db_user = env_or("ACTEBLE_DB_USER", DEFAULT_DB_USER);
    Ok(RotateService::new(
        ports,
        Arc::new(crate::deploy_cli::service()?),
        db_user,
    ))
}

/// NAME and TARGET, `--dry-run`, `--yes`; `None` for anything else.
fn parse(args: &[String]) -> Option<(&str, &str, bool, bool)> {
    let flag = |f: &str| args.iter().any(|a| a == f);
    let words: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| !matches!(*a, "--dry-run" | "--yes"))
        .collect();
    match words.as_slice() {
        [name, target] if !name.starts_with('-') && !target.starts_with('-') => {
            Some((name, target, flag("--dry-run"), flag("--yes")))
        }
        _ => None,
    }
}

pub async fn run_rotate(args: &[String]) -> CliResult {
    let Some((name, word, dry_run, yes)) = parse(args) else {
        eprintln!("{USAGE}");
        return Ok(ExitCode::from(2));
    };
    let target = match RotateTarget::parse(word) {
        Ok(target) => target,
        Err(e) => return finish(name, Err(e)),
    };
    if !dry_run && !yes {
        return finish(
            name,
            Err(DomainError::ValidationError(
                "rotate changes production credentials and redeploys: run it again with --yes, or with --dry-run to see the plan".into(),
            )),
        );
    }
    finish(
        name,
        service()?.rotate(name, target, &api_dir()?, dry_run).await,
    )
}
