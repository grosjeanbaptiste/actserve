//! Podman commands shared by the services (rootless, the ssh login user).
use crate::domain::deploy::GHCR_USER;

/// Runs `script` logged in to ghcr.io (the token arrives on stdin) and logs
/// out whether the script succeeded or not, so no credential stays on disk.
pub fn ghcr_wrapped(script: &str) -> String {
    format!(
        "podman login ghcr.io -u '{GHCR_USER}' --password-stdin >/dev/null || exit 1\n\
         (\n{script}\n)\nrc=$?\npodman logout ghcr.io >/dev/null 2>&1\nexit $rc"
    )
}
