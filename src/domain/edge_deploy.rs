//! The edge deploy: what is copied and the commands run on the server.
use crate::domain::observe::{EDGE_DIR, EDGE_ENV_PATH};

pub use crate::domain::observe::EDGE_SECRET;

/// What is copied from the edge repository to the server (`sites` and `quadlet` are directories).
/// `Caddyfile.socket` is the one Caddy reads when systemd hands it the sockets.
pub const EDGE_FILES: [&str; 4] = ["Caddyfile", "Caddyfile.socket", "sites", "quadlet"];
pub const EDGE_IMAGE: &str = "ghcr.io/acteble/acteble_edge:latest";

/// Where Quadlet reads the user units of the login user (rootless Podman).
const UNIT_DIR: &str = "$HOME/.config/containers/systemd";
/// Where the user's systemd reads plain units: the `.socket` is not a Quadlet unit.
const SOCKET_DIR: &str = "$HOME/.config/systemd/user";

pub fn prepare_command() -> String {
    format!("mkdir -p '{EDGE_DIR}' \"{UNIT_DIR}\" \"{SOCKET_DIR}\"")
}

/// Installs the copied units: Quadlet ones where Quadlet reads them, the socket where
/// systemd reads it. A missing kind makes `cp` fail, so the step fails loudly.
pub fn units_command() -> String {
    let q = format!("'{EDGE_DIR}/quadlet'");
    format!(
        "cp {q}/*.container {q}/*.network {q}/*.volume \"{UNIT_DIR}/\" && cp {q}/*.socket \"{SOCKET_DIR}/\""
    )
}

/// Takes the `/edge` binary out of the image; no socket is mounted. The
/// temporary container is removed whether the copy worked or not.
pub fn binary_script() -> String {
    format!(
        "podman pull {EDGE_IMAGE} && id=$(podman create {EDGE_IMAGE} /edge) && podman cp $id:/edge {EDGE_DIR}/edge; rc=$?; podman rm $id >/dev/null 2>&1; exit $rc"
    )
}

/// Keeps the config (domains only) where the Caddy unit reads it (`EnvironmentFile=`),
/// then lets the edge enable the sites, reload the units and start or reload Caddy.
/// The config arrives on the server's tmpfs; the caller removes that copy.
pub fn start_command() -> String {
    format!(
        "install -m 600 {EDGE_ENV_PATH} '{EDGE_DIR}/.env' && cd '{EDGE_DIR}' && ./edge up --runtime podman --env-file {EDGE_DIR}/.env"
    )
}
