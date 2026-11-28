//! The API deploy names (secrets, env file, image) shared by the Podman deploy.

pub const API_SECRET: &str = "acteble-api-prod";
pub const API_ENV_PATH: &str = "/dev/shm/acteble-api.env";
pub const GHCR_SECRET: &str = "ghcr-read-packages";
pub(crate) const GHCR_USER: &str = "grosjeanbaptiste";
pub(crate) const IMAGE_REPO: &str = "ghcr.io/acteble/acteble_api";
/// A GHCR tag: letters, digits, `_`, `-`, `.`; never leading `-` or `.`.
pub fn valid_image_tag(tag: &str) -> bool {
    !tag.is_empty()
        && !tag.starts_with(['-', '.'])
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}
