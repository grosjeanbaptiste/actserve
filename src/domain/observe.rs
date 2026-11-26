//! What a stack's containers are doing: the commands `ps` and `logs` run on the server
//! (rootless Podman, Quadlet units in the user session).
use crate::domain::errors::DomainError;

/// A Quadlet stack on a server: units `acteble_<stack>-<service>`, containers `systemd-acteble_<stack>-…`.
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

    fn name(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Cdn => "cdn",
        }
    }

    pub fn default_service(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Cdn => "cdn_checkpoint",
        }
    }
}

/// A service name: letters, digits, `_`, `-`, `.`; never leading `-`.
/// Anything else could break the single quotes it is placed in.
pub fn valid_service(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

pub fn ps_command(stack: Stack) -> String {
    format!("podman ps --filter name=systemd-acteble_{}", stack.name())
}

/// What the whole server is using: cpus, load, memory, disk, and what each
/// container holds. Read-only: nothing here changes the server.
pub fn resources_command() -> &'static str {
    "echo cpus; nproc; echo load; uptime; echo memory; free -m; echo disk; df -h /; \
     echo containers; podman stats --no-stream --format '{{.Name}} {{.MemUsage}}'"
}

pub fn logs_command(stack: Stack, service: &str) -> Result<String, DomainError> {
    if !valid_service(service) {
        return Err(DomainError::ValidationError(format!(
            "invalid service name {service:?}"
        )));
    }
    let name = stack.name();
    let service = service.strip_prefix(&format!("{name}_")).unwrap_or(service);
    let unit = service.replace('_', "-");
    Ok(format!(
        "journalctl --user -u acteble_{name}-{unit}.service -f -n 100"
    ))
}

pub const EDGE_DIR: &str = "/opt/acteble/edge";
pub const EDGE_SECRET: &str = "acteble-edge-prod";
pub const EDGE_ENV_PATH: &str = "/dev/shm/acteble-edge.env";

/// What the edge says about itself, read with its env file.
pub fn edge_status_command() -> String {
    format!("cd '{EDGE_DIR}' && ./edge status --runtime podman --env-file {EDGE_ENV_PATH}")
}
