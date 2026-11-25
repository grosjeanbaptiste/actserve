//! The geo build on the CDN host: stages and the commands run on the server.
use crate::domain::{cdn::CDN_ENV_PATH, errors::DomainError};

/// Name of the detached build container (kept from `acteble cdn-build-geo`).
pub const GEO_BUILD_NAME: &str = "acteble-geo-build";
/// The secret key that signs the geo manifest; a build without it is refused.
pub const SIGNING_KEY: &str = "CDN_MANIFEST_SIGNING_KEY";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoStage {
    Catalog,
    Tree,
    Publish,
    All,
}

impl GeoStage {
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s {
            "catalog" => Ok(Self::Catalog),
            "tree" => Ok(Self::Tree),
            "publish" => Ok(Self::Publish),
            "all" => Ok(Self::All),
            other => Err(DomainError::ValidationError(format!(
                "cdn-build-geo: stage {other:?} must be catalog|tree|publish|all"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Catalog => "catalog",
            Self::Tree => "tree",
            Self::Publish => "publish",
            Self::All => "all",
        }
    }
}

const GEO_IMAGE: &str = "ghcr.io/acteble/acteble_geo:latest";
/// Same limits as `acteble_cdn-geo-builder.container` of the CDN repository.
const RUN_FLAGS: &str = "--network acteble_cdn --cap-drop ALL --security-opt no-new-privileges --memory 2800m --ulimit nofile=262144:262144 -v acteble_cdn_geo_work:/srv/acteble_cdn/geo-out";
const STORAGE_ENV: &str = "-e APP__STORAGE__MINIO__ENDPOINT=cdn-minio:9000 -e APP__STORAGE__MINIO__BUCKET=acteble-files -e APP__STORAGE__MINIO__SECURE=false -e APP__STORAGE__MINIO__REGION=us-east-1";
const SIGNING_SECRET: &str = "acteble_cdn_manifest_signing_key";

fn secret_flags() -> String {
    [
        (
            "acteble_cdn_minio_root_user",
            "APP__STORAGE__MINIO__ACCESS_KEY",
        ),
        (
            "acteble_cdn_minio_root_password",
            "APP__STORAGE__MINIO__SECRET_KEY",
        ),
        (SIGNING_SECRET, "GEO_MANIFEST_SIGNING_KEY"),
        (SIGNING_SECRET, "CDN_MANIFEST_SIGNING_KEY"),
    ]
    .map(|(name, target)| format!("--secret {name},type=env,target={target}"))
    .join(" ")
}

/// Rootless Podman, no Docker. The env file (on the server tmpfs) holds the signing key,
/// which becomes a podman secret; the image is pulled, the build starts detached (it
/// keeps running if the link drops). No value is ever an argument.
pub fn build_script(stage: GeoStage) -> String {
    format!(
        "set -a && . {CDN_ENV_PATH} && set +a\n\
         printf %s \"${SIGNING_KEY}\" | podman secret create --replace {SIGNING_SECRET} - >/dev/null || exit 1\n\
         podman pull {GEO_IMAGE} && podman run -d --name {GEO_BUILD_NAME} {RUN_FLAGS} {STORAGE_ENV} {secrets} {GEO_IMAGE} --stage {stage}",
        secrets = secret_flags(),
        stage = stage.as_str(),
    )
}

/// Streams the build's logs, waits for its exit, removes the container, exits with its status.
pub fn follow_command() -> String {
    format!(
        "podman logs -f {GEO_BUILD_NAME}; rc=$(podman wait {GEO_BUILD_NAME}); podman rm {GEO_BUILD_NAME} >/dev/null; exit $rc"
    )
}

/// True when `env` (KEY=VALUE lines) sets `key` to a non-empty value.
pub fn env_has_value(env: &str, key: &str) -> bool {
    env.lines()
        .filter_map(|l| l.strip_prefix(key)?.strip_prefix('='))
        .any(|v| !v.is_empty())
}
