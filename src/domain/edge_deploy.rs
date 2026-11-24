//! The edge deploy: what is copied and the commands run on the server.
use crate::domain::observe::{EDGE_DIR, EDGE_ENV_PATH};

pub use crate::domain::observe::EDGE_SECRET;

/// What is copied from the edge repository to the server (`sites` is a directory).
pub const EDGE_FILES: [&str; 3] = ["Caddyfile", "compose.yml", "sites"];
pub const EDGE_IMAGE: &str = "ghcr.io/acteble/acteble_edge:latest";

pub fn prepare_command() -> String {
    format!("mkdir -p '{EDGE_DIR}'")
}

/// Takes the `/edge` binary out of the image; no socket is mounted. The
/// temporary container is removed whether the copy worked or not.
pub fn binary_script() -> String {
    format!(
        "docker pull {EDGE_IMAGE} && id=$(docker create {EDGE_IMAGE} /edge) && docker cp $id:/edge {EDGE_DIR}/edge; rc=$?; docker rm $id >/dev/null 2>&1; exit $rc"
    )
}

/// Starts the edge; its config is the env file on the server's tmpfs.
pub fn start_command() -> String {
    format!("cd '{EDGE_DIR}' && ./edge up --env-file {EDGE_ENV_PATH}")
}
