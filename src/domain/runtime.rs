//! The shell commands of the runtime step of a server (step 3 of the
//! bootstrap): Podman (the services are rootless Quadlet units, logging to the
//! journal) and the stack directories. Apply only installs or creates, nothing
//! is ever removed.
use crate::domain::dirs_file::DeclaredDir;

pub const PODMAN_CHECK: &str = "podman --version > /dev/null 2>&1";
pub const PODMAN_APPLY: &str =
    "sudo -n sh -c 'DEBIAN_FRONTEND=noninteractive apt-get install -y podman'";

/// Succeeds when every directory belongs to its declared owner.
pub fn dirs_check_command(dirs: &[DeclaredDir]) -> String {
    dirs.iter()
        .map(|d| format!(r#"test "$(stat -c %U {})" = {}"#, d.path, d.owner))
        .collect::<Vec<_>>()
        .join(" && ")
}

/// Creates each missing directory for its owner, then sets the owner.
pub fn dirs_apply_command(dirs: &[DeclaredDir]) -> String {
    let cmd = dirs
        .iter()
        .map(|DeclaredDir { path: p, owner: o }| {
            format!("{{ [ -d {p} ] || install -d -o {o} -g {o} {p}; }} && chown {o}:{o} {p}")
        })
        .collect::<Vec<_>>()
        .join(" && ");
    format!("sudo -n sh -c '{cmd}'")
}
