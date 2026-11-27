//! The edge deploy: what is copied and the commands run on the server.
use crate::domain::observe::{EDGE_DIR, EDGE_ENV_PATH};

pub use crate::domain::observe::EDGE_SECRET;

/// What is copied from the edge repository to the server (`sites` and `quadlet` are directories).
pub const EDGE_FILES: [&str; 4] = ["Caddyfile", "Caddyfile.socket", "sites", "quadlet"];
pub const EDGE_IMAGE: &str = "ghcr.io/acteble/acteble_edge:latest";

/// Where Quadlet reads the user units of the login user (rootless Podman).
const UNIT_DIR: &str = "$HOME/.config/containers/systemd";
/// Where systemd reads plain user units: Quadlet ignores `.socket` files.
const SOCKET_DIR: &str = "$HOME/.config/systemd/user";

pub fn prepare_command() -> String {
    format!("mkdir -p '{EDGE_DIR}' \"{UNIT_DIR}\" \"{SOCKET_DIR}\"")
}

/// Installs the copied units: `.socket` files to systemd, the others to Quadlet.
/// Nothing is enabled here; `edge up` starts the sockets.
pub fn units_command() -> String {
    format!(
        "for f in '{EDGE_DIR}/quadlet'/*; do case \"$f\" in *.socket) cp \"$f\" \"{SOCKET_DIR}/\";; *) cp \"$f\" \"{UNIT_DIR}/\";; esac; done"
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
