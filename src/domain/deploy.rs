//! The API deploy and rollback: names, files and the commands run on the server.
use crate::domain::{errors::DomainError, observe::Stack};

pub const API_SECRET: &str = "acteble-api-prod";
pub const API_ENV_PATH: &str = "/dev/shm/acteble-api.env";
pub const GHCR_SECRET: &str = "ghcr-read-packages";
pub(crate) const GHCR_USER: &str = "grosjeanbaptiste";
const IMAGE_REPO: &str = "ghcr.io/acteble/acteble_api";
const COMPOSE_BASE: &str =
    "-f docker-compose.yml -f docker-compose.prod.yml -f docker-compose.ghcr.yml";

/// What is copied from the API repository to the server before a deploy.
pub const API_COMPOSE_FILES: [&str; 4] = [
    "docker-compose.yml",
    "docker-compose.prod.yml",
    "docker-compose.ghcr.yml",
    "Caddyfile",
];

/// Runs `script` logged in to ghcr.io (the token arrives on stdin) and logs
/// out whether the script succeeded or not, so no credential stays on disk.
pub fn ghcr_wrapped(script: &str) -> String {
    format!(
        "docker login ghcr.io -u '{GHCR_USER}' --password-stdin >/dev/null || exit 1\n\
         (\n{script}\n)\nrc=$?\ndocker logout ghcr.io >/dev/null 2>&1\nexit $rc"
    )
}

pub(crate) fn compose(extra: &str) -> String {
    format!("docker compose {COMPOSE_BASE} --env-file {API_ENV_PATH} {extra}")
}

/// Keep the image being replaced as `:previous`, pull `:latest`, restart, prune.
pub fn deploy_script() -> String {
    format!(
        "cd '{dir}'\n\
         docker image tag '{IMAGE_REPO}:latest' '{IMAGE_REPO}:previous' || true\n\
         {pull}\n\
         {up}\n\
         docker image prune -f",
        dir = Stack::Api.dir(),
        pull = compose("pull api"),
        up = compose("up -d --no-build --remove-orphans"),
    )
}

/// A GHCR tag: letters, digits, `_`, `-`, `.`; never leading `-` or `.`.
pub fn valid_image_tag(tag: &str) -> bool {
    !tag.is_empty()
        && !tag.starts_with(['-', '.'])
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Restart the API on `tag`. `previous` is already on the server (tagged by
/// the last deploy); any other tag is pulled first.
pub fn rollback_script(tag: &str) -> Result<String, DomainError> {
    if !valid_image_tag(tag) {
        return Err(DomainError::ValidationError(format!(
            "invalid image tag {tag:?}"
        )));
    }
    let image = format!("{IMAGE_REPO}:{tag}");
    let pull = if tag == "previous" {
        String::new()
    } else {
        format!("API_IMAGE='{image}' {}\n", compose("pull api"))
    };
    Ok(format!(
        "cd '{dir}'\n{pull}API_IMAGE='{image}' {up}",
        dir = Stack::Api.dir(),
        up = compose("up -d --no-build"),
    ))
}
