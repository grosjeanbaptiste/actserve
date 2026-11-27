//! The CDN stack deploy: names and the commands run on the server. The stack
//! is made of rootless Podman Quadlet units of the user session, not Compose.
use crate::domain::{
    observe::Stack,
    stacks::{NETWORK_APPLY, NETWORK_CHECK},
};

pub const CDN_SECRET: &str = "acteble-cdn-prod";
pub const CDN_ENV_PATH: &str = "/dev/shm/acteble-cdn.env";
/// The Quadlet files copied from the CDN repository into the user's unit directory (relative to
/// the home of the server's user: `scp` resolves it there).
pub const CDN_UNIT_DIR: &str = ".config/containers/systemd";
pub const CDN_UNIT_FILES: [&str; 9] = [
    "quadlet/acteble_cdn.network",
    "quadlet/acteble_cdn_minio.volume",
    "quadlet/acteble_cdn_geo_work.volume",
    "quadlet/acteble_cdn-minio.kube",
    "quadlet/acteble_cdn-minio.yaml",
    "quadlet/acteble_cdn-checkpoint.kube",
    "quadlet/acteble_cdn-checkpoint.yaml",
    "quadlet/acteble_cdn-publisher.container",
    "quadlet/acteble_cdn-geo-builder.container",
];
/// The health script the MinIO unit mounts from the stack directory.
pub const CDN_HEALTH_SCRIPT: &str = "quadlet/minio-health.sh";
const CDN_IMAGE: &str = "ghcr.io/acteble/acteble_cdn:latest";
const MC_IMAGE: &str = "cgr.dev/chainguard/minio-client@sha256:10388d580a724f356b94c9b2631b5a179039cabe8c76e9b37d86ed007d569b05";
const GEO_PREFIX: &str = "local/acteble-files/geo";

pub fn prepare_command() -> String {
    format!(
        "mkdir -p '{}' && mkdir -p \"$HOME/{CDN_UNIT_DIR}\"",
        Stack::Cdn.dir()
    )
}

/// Drops the `.container` units that the `.kube` units replace (two files for one service name
/// make Quadlet refuse both), loads the copied units, then the network the edge and the CDN
/// share: started (which creates it) only when absent.
pub fn network_command() -> String {
    format!(
        "rm -f \"$HOME/{CDN_UNIT_DIR}/acteble_cdn-minio.container\" \"$HOME/{CDN_UNIT_DIR}/acteble_cdn-checkpoint.container\" && systemctl --user daemon-reload && {{ {NETWORK_CHECK} || {NETWORK_APPLY}; }}"
    )
}

/// Pull the image, put the env file's values in Podman secrets (read by the units, never in a
/// unit file; `acteble-cdn` is a Kubernetes `v1.Secret` for the `.kube` units, the others are
/// plain values for the one-shot units), then restart the units. The env file is on the server's tmpfs.
pub fn start_script() -> String {
    format!(
        "set -e\n\
         set -a && . {CDN_ENV_PATH} && set +a\n\
         podman pull {CDN_IMAGE}\n\
         secret() {{ printf %s \"$2\" | podman secret create --replace \"$1\" -; }}\n\
         b64() {{ printf %s \"$1\" | base64 | tr -d '\\n'; }}\n\
         printf 'apiVersion: v1\\nkind: Secret\\nmetadata:\\n  name: acteble-cdn\\ndata:\\n  minio_root_user: %s\\n  minio_root_password: %s\\n  token_secret: %s\\n  manifest_publisher_key_id: %s\\n' \"$(b64 \"$MINIO_ROOT_USER\")\" \"$(b64 \"$MINIO_ROOT_PASSWORD\")\" \"$(b64 \"$CDN_TOKEN_SECRET\")\" \"$(b64 \"$CDN_MANIFEST_PUBLISHER_KEY_ID\")\" | podman secret create --replace acteble-cdn -\n\
         secret acteble_cdn_minio_root_user \"$MINIO_ROOT_USER\"\n\
         secret acteble_cdn_minio_root_password \"$MINIO_ROOT_PASSWORD\"\n\
         secret acteble_cdn_token_secret \"$CDN_TOKEN_SECRET\"\n\
         secret acteble_cdn_manifest_publisher_key_id \"$CDN_MANIFEST_PUBLISHER_KEY_ID\"\n\
         [ -z \"$CDN_MANIFEST_SIGNING_KEY\" ] || secret acteble_cdn_manifest_signing_key \"$CDN_MANIFEST_SIGNING_KEY\"\n\
         systemctl --user daemon-reload\n\
         systemctl --user restart acteble_cdn-minio.service acteble_cdn-checkpoint.service"
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
         podman run --rm --network acteble_cdn -e MC_HOST_local --cap-drop ALL --read-only --tmpfs /tmp {MC_IMAGE} --config-dir /tmp/.mc anonymous set download {GEO_PREFIX} && exit 0\n\
         sleep 3\n\
         done\n\
         exit 1"
    )
}
