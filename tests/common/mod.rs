//! Fixtures shared by the integration tests of the API deploy.
use actserve::domain::api_podman::{API_SECRETS, API_SETTINGS};

/// A prod env holding every required key of the API, each set to `v`.
pub fn full_api_env() -> String {
    let secrets = API_SECRETS.iter().map(|(k, _)| *k);
    let required = API_SETTINGS.iter().filter(|s| s.2.is_none()).map(|s| s.0);
    secrets
        .chain(required)
        .map(|k| format!("{k}=v\n"))
        .collect()
}
