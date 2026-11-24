//! The CDN stack deploy: names and the commands run on the server.
use crate::domain::{
    observe::Stack,
    stacks::{NETWORK_APPLY, NETWORK_CHECK},
};

pub const CDN_SECRET: &str = "acteble-cdn-prod";
pub const CDN_ENV_PATH: &str = "/dev/shm/acteble-cdn.env";
/// What is copied from the CDN repository to the server.
pub const CDN_COMPOSE_FILE: &str = "compose.yml";
const MC_IMAGE: &str = "cgr.dev/chainguard/minio-client@sha256:10388d580a724f356b94c9b2631b5a179039cabe8c76e9b37d86ed007d569b05";
const GEO_PREFIX: &str = "local/acteble-files/geo";

pub fn prepare_command() -> String {
    format!("mkdir -p '{}'", Stack::Cdn.dir())
}

/// The network the edge and the CDN share: created only when absent.
pub fn network_command() -> String {
    format!("{NETWORK_CHECK} || {NETWORK_APPLY}")
}

/// Pull, then bring the stack up; the env file is on the server's tmpfs.
pub fn start_script() -> String {
    format!(
        "cd '{dir}'\n\
         docker compose --env-file {CDN_ENV_PATH} pull && docker compose --env-file {CDN_ENV_PATH} up -d --remove-orphans",
        dir = Stack::Cdn.dir(),
    )
}

/// Opens anonymous download on the geo prefix only. The credentials come from
/// the env file into the environment of a throwaway read-only client on the
/// stack's network; none is an argument. Retried: the store may still be starting.
pub fn geo_script() -> String {
    format!(
        "set -a && . {CDN_ENV_PATH} && set +a\n\
         export MC_HOST_local=\"http://$MINIO_ROOT_USER:$MINIO_ROOT_PASSWORD@cdn-minio:9000\"\n\
         for i in 1 2 3 4 5 6 7 8 9 10; do\n\
         docker run --rm --network acteble_cdn -e MC_HOST_local --cap-drop ALL --read-only --tmpfs /tmp {MC_IMAGE} --config-dir /tmp/.mc anonymous set download {GEO_PREFIX} && exit 0\n\
         sleep 3\n\
         done\n\
         exit 1"
    )
}
