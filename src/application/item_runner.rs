//! The check-then-apply engine shared by the steps of the bootstrap: an item
//! is checked, and changed only when its check fails (never in a dry run).
use crate::domain::fleet::FleetHost;
use crate::domain::{
    errors::DomainError,
    harden::{ItemOutcome, ItemReport},
};
use crate::ports::outbound::{
    remote_shell::{RemoteOutput, RemoteShellPort},
    server_inventory::ServerInventoryPort,
};

/// A command and what it reads on stdin.
pub struct Cmd<'a>(pub &'a str, pub Option<&'a str>);

/// How a failed remote command reads in a report.
pub fn failed(out: &RemoteOutput) -> String {
    let code = out.code.map_or("signal".to_string(), |c| c.to_string());
    format!("exit {code}: {}", out.stderr.trim())
}

/// `DomainError`'s Display drops the details of `ExecutionFailed`; the operator
/// needs them (ssh's own message, the tool's exit and stderr).
pub fn describe(e: &DomainError) -> String {
    match e {
        DomainError::ExecutionFailed {
            reason,
            details: Some(d),
        } => format!("{reason}: {d}"),
        other => other.to_string(),
    }
}

/// Check; when it fails, apply (or report `WouldApply` in a dry run).
pub async fn check_apply(
    shell: &dyn RemoteShellPort,
    target: &str,
    check: Cmd<'_>,
    apply: Cmd<'_>,
    dry_run: bool,
) -> Result<ItemOutcome, DomainError> {
    if shell.run(target, check.0, check.1).await?.code == Some(0) {
        return Ok(ItemOutcome::Ok);
    }
    if dry_run {
        return Ok(ItemOutcome::WouldApply);
    }
    let done = shell.run(target, apply.0, apply.1).await?;
    Ok(if done.code == Some(0) {
        ItemOutcome::Applied
    } else {
        ItemOutcome::Failed(failed(&done))
    })
}

/// Records the outcome; true when the run may go on.
pub fn push(
    items: &mut Vec<ItemReport>,
    step: &'static str,
    item: &'static str,
    outcome: ItemOutcome,
) -> bool {
    let go = !matches!(outcome, ItemOutcome::Failed(_));
    items.push(ItemReport {
        step,
        item,
        outcome,
    });
    go
}

/// The inventory host called `name`, or `NotFound`.
pub async fn find_host(
    inventory: &dyn ServerInventoryPort,
    name: &str,
) -> Result<FleetHost, DomainError> {
    inventory
        .hosts()
        .await?
        .into_iter()
        .find(|h| h.name() == name)
        .ok_or(DomainError::NotFound {
            entity: "FleetHost",
            id: name.to_string(),
        })
}
