//! The shell commands of the runtime step of a server (step 3 of the
//! bootstrap): Docker with Compose, bounded container logs, the stack
//! directories. Apply only installs or creates, nothing is ever removed.
use crate::domain::dirs_file::DeclaredDir;

pub const DOCKER_CHECK: &str = "docker compose version > /dev/null 2>&1";
// ponytail: needs Docker's apt repository already configured (blank servers
// get it with the provider image or by hand); it fails, reported, when absent.
pub const DOCKER_APPLY: &str = "sudo -n sh -c 'DEBIAN_FRONTEND=noninteractive apt-get install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin'";

/// Succeeds when the daemon.json on the server equals the declared one (stdin).
pub const LOGS_CHECK: &str = "sudo -n cmp -s - /etc/docker/daemon.json";
/// Installs the declared daemon.json (stdin). Docker reads it only at start and
/// restarting it stops every container, so it takes effect at the next restart.
pub const LOGS_APPLY: &str = "sudo -n sh -c 'mkdir -p /etc/docker && cat > /etc/docker/daemon.json.new && mv /etc/docker/daemon.json.new /etc/docker/daemon.json'";

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
