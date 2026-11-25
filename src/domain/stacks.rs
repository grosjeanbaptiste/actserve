//! The edge and the stacks of a server (steps 4 and 5 of the bootstrap).
use crate::domain::errors::DomainError;

pub const NETWORK_CHECK: &str = "podman network exists acteble_edge";
/// The network is a Quadlet unit of the edge: starting it creates the network.
pub const NETWORK_APPLY: &str = "systemctl --user start acteble_edge-network.service";
pub const CADDY_CHECK: &str = "systemctl --user is-active --quiet acteble_edge-caddy.service";

/// Succeeds when the units of the cdn stack (the object store and the checkpoint) are active.
pub fn cdn_check_command() -> &'static str {
    "systemctl --user is-active --quiet acteble_cdn-minio.service && systemctl --user is-active --quiet acteble_cdn-checkpoint.service"
}

/// Result of asking for a stack or the edge to be started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeployOutcome {
    Started,
    /// Could not be started; the reason is shown to the operator.
    Unavailable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackMode {
    /// Started when it is not running.
    Run,
    /// Named and left alone.
    Held,
}

/// One line of `servers/<name>/stacks.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredStack {
    pub name: String,
    pub mode: StackMode,
}

/// The stacks of `servers/<name>/stacks.txt` (`NAME|MODE`), in file order.
/// NAME is a stack this code knows (`cdn`, `api`), MODE is `run` or `held`;
/// `api` can only be held, since it has no running check and its deploy is
/// frozen. The edge is step 4, not a stack. Every problem is reported in one
/// pass as `<file>:<line>: …`.
pub fn parse_stacks(file: &str, text: &str) -> Result<Vec<DeclaredStack>, DomainError> {
    let mut stacks = Vec::new();
    let mut problems = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = i + 1;
        let parsed = match line.split_once('|') {
            Some((name @ ("cdn" | "api"), mode @ ("run" | "held"))) => Some((
                name,
                if mode == "run" {
                    StackMode::Run
                } else {
                    StackMode::Held
                },
            )),
            _ => None,
        };
        match parsed {
            None => problems.push(format!(
                "{file}:{at}: \"{line}\" must be NAME|MODE, NAME cdn or api (the edge is step 4), MODE run or held"
            )),
            Some(("api", StackMode::Run)) => problems.push(format!(
                "{file}:{at}: api can only be held: it has no running check and its deploy is frozen until the test suites are green"
            )),
            Some((name, mode)) => stacks.push(DeclaredStack { name: name.into(), mode }),
        }
    }
    if problems.is_empty() {
        Ok(stacks)
    } else {
        Err(DomainError::ValidationError(problems.join("\n")))
    }
}
