//! The geo build on the CDN host: stages and the commands run on the server.
use crate::domain::{cdn::CDN_ENV_PATH, errors::DomainError, observe::Stack};

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

/// Pull the builder image, then start it detached (it keeps running if the link drops).
pub fn build_script(stage: GeoStage) -> String {
    let run = format!("docker compose --env-file {CDN_ENV_PATH} --profile tools");
    format!(
        "cd '{dir}'\n{run} pull geo_builder && {run} run -d --name {GEO_BUILD_NAME} geo_builder --stage {stage}",
        dir = Stack::Cdn.dir(),
        stage = stage.as_str(),
    )
}

/// Streams the build's logs, waits for its exit, removes the container, exits with its status.
pub fn follow_command() -> String {
    format!(
        "docker logs -f {GEO_BUILD_NAME}; rc=$(docker wait {GEO_BUILD_NAME}); docker rm {GEO_BUILD_NAME} >/dev/null; exit $rc"
    )
}

/// True when `env` (KEY=VALUE lines) sets `key` to a non-empty value.
pub fn env_has_value(env: &str, key: &str) -> bool {
    env.lines()
        .filter_map(|l| l.strip_prefix(key)?.strip_prefix('='))
        .any(|v| !v.is_empty())
}
