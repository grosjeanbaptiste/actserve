//! What a stack's containers are doing: the commands `ps` and `logs` run on the server.
use crate::domain::errors::DomainError;

/// A compose stack that lives under `/opt/acteble/` on a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stack {
    Api,
    Cdn,
}

impl Stack {
    pub fn parse(name: &str) -> Result<Self, DomainError> {
        match name {
            "api" => Ok(Self::Api),
            "cdn" => Ok(Self::Cdn),
            other => Err(DomainError::ValidationError(format!(
                "unknown stack {other:?}: expected api or cdn"
            ))),
        }
    }

    pub fn dir(self) -> &'static str {
        match self {
            Self::Api => "/opt/acteble/api",
            Self::Cdn => "/opt/acteble/cdn",
        }
    }

    pub fn default_service(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Cdn => "cdn_checkpoint",
        }
    }
}

/// A compose service name: letters, digits, `_`, `-`, `.`; never leading `-`.
/// Anything else could break the single quotes it is placed in.
pub fn valid_service(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

pub fn ps_command(stack: Stack) -> String {
    format!("cd '{}' && docker compose ps", stack.dir())
}

pub fn logs_command(stack: Stack, service: &str) -> Result<String, DomainError> {
    if !valid_service(service) {
        return Err(DomainError::ValidationError(format!(
            "invalid service name {service:?}"
        )));
    }
    Ok(format!(
        "cd '{}' && docker compose logs --follow --tail=100 '{service}'",
        stack.dir()
    ))
}

pub const EDGE_DIR: &str = "/opt/acteble/edge";
pub const EDGE_SECRET: &str = "acteble-edge-prod";
pub const EDGE_ENV_PATH: &str = "/dev/shm/acteble-edge.env";

/// What the edge says about itself, read with its env file.
pub fn edge_status_command() -> String {
    format!("cd '{EDGE_DIR}' && ./edge status --env-file {EDGE_ENV_PATH}")
}
