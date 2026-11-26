//! `actserve ps NAME STACK` and `actserve logs NAME STACK [SERVICE]`.
use std::process::ExitCode;
use std::sync::Arc;

use crate::adapters::outbound::system::{
    actesskey_secret_store::ActesskeySecretStore, ssh_hostname_probe::SshHostnameProbe,
    ssh_remote_shell::SshRemoteShell, ssh_terminal::SshTerminal,
};
use crate::application::edge_status_service::EdgeStatusService;
use crate::application::observe_service::{ObserveRun, ObserveService, exit_code, render};
use crate::domain::{errors::DomainError, observe::Stack};
use crate::fleet_cli::inventory_and_ssh;

type CliResult = Result<ExitCode, Box<dyn std::error::Error>>;

const PS_USAGE: &str = "usage: actserve ps NAME api|cdn";
const LOGS_USAGE: &str = "usage: actserve logs NAME api|cdn [SERVICE]";

fn service() -> Result<ObserveService, Box<dyn std::error::Error>> {
    let (inventory, ssh) = inventory_and_ssh()?;
    Ok(ObserveService::new(
        inventory,
        Arc::new(SshHostnameProbe::new(ssh.clone())),
        Arc::new(SshRemoteShell::new(ssh.clone())),
        Arc::new(SshTerminal::new(ssh)),
    ))
}

/// Prints the run and maps it to its exit code: the command's own, 3 for a
/// refused host, 2 for an unknown name, 1 for a stack or service name refused.
fn finish(name: &str, run: Result<ObserveRun, DomainError>) -> CliResult {
    match run {
        Ok(run) => {
            print!("{}", render(name, &run));
            if let ObserveRun::Output(o) = &run {
                eprint!("{}", o.stderr);
            }
            Ok(ExitCode::from(exit_code(&run)))
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

fn is_name(s: &str) -> bool {
    !s.starts_with('-')
}

/// The state of a stack's containers (`docker compose ps`).
pub async fn run_ps(args: &[String]) -> CliResult {
    let [name, stack] = args else {
        eprintln!("{PS_USAGE}");
        return Ok(ExitCode::from(2));
    };
    if !is_name(name) {
        eprintln!("{PS_USAGE}");
        return Ok(ExitCode::from(2));
    }
    let run = match Stack::parse(stack) {
        Ok(stack) => service()?.ps(name, stack).await,
        Err(e) => Err(e),
    };
    finish(name, run)
}

const RESOURCES_USAGE: &str = "usage: actserve resources NAME";

/// What the whole server is using: cpus, load, memory, disk, memory per container.
pub async fn run_resources(args: &[String]) -> CliResult {
    let [name] = args else {
        eprintln!("{RESOURCES_USAGE}");
        return Ok(ExitCode::from(2));
    };
    if !is_name(name) {
        eprintln!("{RESOURCES_USAGE}");
        return Ok(ExitCode::from(2));
    }
    finish(name, service()?.resources(name).await)
}

/// Follows the logs of a stack's service on the operator's terminal.
pub async fn run_logs(args: &[String]) -> CliResult {
    let (name, stack, svc) = match args {
        [n, s] => (n, s, None),
        [n, s, v] => (n, s, Some(v.as_str())),
        _ => {
            eprintln!("{LOGS_USAGE}");
            return Ok(ExitCode::from(2));
        }
    };
    if !is_name(name) {
        eprintln!("{LOGS_USAGE}");
        return Ok(ExitCode::from(2));
    }
    let run = match Stack::parse(stack) {
        Ok(stack) => service()?.logs(name, stack, svc).await,
        Err(e) => Err(e),
    };
    finish(name, run)
}

const EDGE_STATUS_USAGE: &str = "usage: actserve edge-status NAME";

/// What the edge of a server says about itself; its secret comes from actesskey
/// (`$ACTSERVE_ACTESSKEY`, default `actesskey`).
pub async fn run_edge_status(args: &[String]) -> CliResult {
    let [name] = args else {
        eprintln!("{EDGE_STATUS_USAGE}");
        return Ok(ExitCode::from(2));
    };
    if !is_name(name) {
        eprintln!("{EDGE_STATUS_USAGE}");
        return Ok(ExitCode::from(2));
    }
    let (inventory, ssh) = inventory_and_ssh()?;
    let key = std::env::var("ACTSERVE_ACTESSKEY").unwrap_or_else(|_| "actesskey".to_string());
    let service = EdgeStatusService::new(
        inventory,
        Arc::new(SshHostnameProbe::new(ssh.clone())),
        Arc::new(SshRemoteShell::new(ssh)),
        Arc::new(ActesskeySecretStore::new(key)),
    );
    finish(name, service.status(name).await)
}
