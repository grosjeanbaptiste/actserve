//! The API stack deploy: rootless Podman Quadlet units of the ssh user, . Names, files
//! and the commands run on the server.
use crate::domain::{
    deploy::{API_ENV_PATH, IMAGE_REPO, valid_image_tag},
    errors::DomainError,
    observe::Stack,
};

/// Where Quadlet reads the units (relative to the home of the server's user: `scp` resolves it there).
pub const API_UNIT_DIR: &str = ".config/containers/systemd";
/// The Quadlet files copied from the API repository, all into `API_UNIT_DIR`.
pub const API_UNIT_FILES: [&str; 10] = [
    "server/quadlet/acteble_api.network",
    "server/quadlet/acteble_api_postgres.volume",
    "server/quadlet/acteble_api_minio.volume",
    "server/quadlet/acteble_api_redis.volume",
    "server/quadlet/acteble_api_pg_backups.volume",
    "server/quadlet/acteble_api-postgres.container",
    "server/quadlet/acteble_api-redis.container",
    "server/quadlet/acteble_api-minio.container",
    "server/quadlet/acteble_api-pg-backup.container",
    "server/quadlet/acteble_api-api.container",
];

/// Key of the prod env, Podman secret the unit reads it from (`Secret=` lines of `acteble_api-api.container`).
pub const API_SECRETS: [(&str, &str); 16] = [
    ("APP_AUTH__JWT_SECRET", "acteble_api_jwt_secret"),
    ("POSTGRES_USER", "acteble_api_postgres_user"),
    ("POSTGRES_PASSWORD", "acteble_api_postgres_password"),
    ("MINIO_ROOT_USER", "acteble_api_minio_root_user"),
    ("MINIO_ROOT_PASSWORD", "acteble_api_minio_root_password"),
    (
        "ACTE_CORPS_SEALING_KEY",
        "acteble_api_acte_corps_sealing_key",
    ),
    ("CDN_TOKEN_SECRET", "acteble_api_cdn_token_secret"),
    ("TURNSTILE_SECRET", "acteble_api_turnstile_secret"),
    (
        "REVENUECAT_WEBHOOK_SECRET",
        "acteble_api_revenuecat_webhook_secret",
    ),
    (
        "REVENUECAT_SECRET_API_KEY",
        "acteble_api_revenuecat_secret_api_key",
    ),
    ("LIVEKIT_API_KEY", "acteble_api_livekit_api_key"),
    ("LIVEKIT_API_SECRET", "acteble_api_livekit_api_secret"),
    (
        "LIVEKIT_EGRESS_ACCESS_KEY",
        "acteble_api_livekit_egress_access_key",
    ),
    ("LIVEKIT_EGRESS_SECRET", "acteble_api_livekit_egress_secret"),
    (
        "PLAY_INTEGRITY_SA_JSON",
        "acteble_api_play_integrity_sa_json",
    ),
    (
        "AGE_ATTESTATION_SIGNING_KEY",
        "acteble_api_age_attestation_signing_key",
    ),
];

/// Key of the prod env, variable of the container (`api.env`), default; no default means required.
pub const API_SETTINGS: [(&str, &str, Option<&str>); 10] = [
    ("CORS_ALLOWED_ORIGINS", "CORS_ALLOWED_ORIGINS", None),
    ("CAPTCHA_ENABLED", "CAPTCHA_ENABLED", Some("true")),
    ("ATTESTATION_ENABLED", "ATTESTATION_ENABLED", Some("false")),
    ("ANDROID_PACKAGE_NAME", "ANDROID_PACKAGE_NAME", Some("")),
    (
        "CDN_MANIFEST_PUBLISHER_KEY_ID",
        "CDN_MANIFEST_PUBLISHER_KEY_ID",
        Some(""),
    ),
    (
        "GEO_OBJECT_BASE",
        "GEO_OBJECT_BASE",
        Some("https://api.acteble.com/"),
    ),
    ("LIVEKIT_URL", "APP__LIVEKIT__URL", None),
    (
        "LIVEKIT_EGRESS_BUCKET",
        "APP__LIVEKIT__EGRESS_STORAGE__BUCKET",
        None,
    ),
    (
        "LIVEKIT_EGRESS_ENDPOINT",
        "APP__LIVEKIT__EGRESS_STORAGE__ENDPOINT",
        None,
    ),
    (
        "LIVEKIT_EGRESS_REGION",
        "APP__LIVEKIT__EGRESS_STORAGE__REGION",
        Some("auto"),
    ),
];

/// Units started (not restarted) on every deploy: the API restarts alone.
const BACKING_UNITS: &str = "acteble_api-postgres.service acteble_api-redis.service \
                             acteble_api-minio.service acteble_api-pg-backup.service";
const API_UNIT: &str = "acteble_api-api.service";

/// Reads `KEY` of the env file `$E` without executing it (a value may hold spaces, quotes or
/// `;`): last occurrence wins, one pair of surrounding quotes is removed.
const HELPERS: &str = r##"val() { sed -n "s/^$1=//p" "$E" | tail -n1 | sed -E "s/^(['\"])(.*)\1\$/\2/"; }
secret() { printf %s "$(val "$1")" | podman secret create --replace "$2" - >/dev/null; }
setting() { v=$(val "$1"); printf '%s=%s\n' "$2" "${v:-$3}"; }"##;

/// The value of `key` in `env`, read as the server's script reads it.
fn value<'a>(env: &'a str, key: &str) -> &'a str {
    let found = env
        .lines()
        .filter_map(|l| l.strip_prefix(key)?.strip_prefix('='))
        .next_back()
        .unwrap_or("");
    let bytes = found.as_bytes();
    match bytes {
        [q @ (b'\'' | b'"'), .., last] if q == last => &found[1..found.len() - 1],
        _ => found,
    }
}

/// The required keys (every secret, every setting without a default) that `env` leaves out or empty.
pub fn missing_env_keys(env: &str) -> Vec<&'static str> {
    let secrets = API_SECRETS.iter().map(|(k, _)| *k);
    let required = API_SETTINGS.iter().filter(|s| s.2.is_none()).map(|s| s.0);
    secrets
        .chain(required)
        .filter(|k| value(env, k).is_empty())
        .collect()
}

pub fn prepare_command() -> String {
    format!("mkdir -p '{}' \"$HOME/{API_UNIT_DIR}\"", Stack::Api.dir())
}

/// Keep the image being replaced as `:previous`, pull `:latest`, put the env file's values in
/// Podman secrets (read by the units) and in `api.env` (the settings), load the units, start the
/// backing ones and restart the API. The env file is on the server's tmpfs.
pub fn start_script() -> String {
    let image = format!("{IMAGE_REPO}:latest");
    let mut s = format!(
        "set -e\nE='{API_ENV_PATH}'\n{HELPERS}\n\
         podman image exists {image} && podman tag {image} {IMAGE_REPO}:previous || true\n\
         podman pull {image}\n"
    );
    for (key, name) in API_SECRETS {
        s.push_str(&format!("secret {key} {name}\n"));
    }
    s.push_str("(umask 077; {\n");
    for (key, var, default) in API_SETTINGS {
        s.push_str(&format!(
            "setting {key} {var} '{}'\n",
            default.unwrap_or("")
        ));
    }
    s.push_str(&format!(
        "}} > '{}/api.env')\n\
         systemctl --user daemon-reload\n\
         systemctl --user start {BACKING_UNITS}\n\
         systemctl --user restart {API_UNIT}\n\
         podman image prune -f",
        Stack::Api.dir()
    ));
    s
}

/// Restart the API on `tag`: retag it `:latest`, which the unit runs. `previous` is already on
/// the server (tagged by the last deploy); any other tag is pulled first.
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
        format!("podman pull {image}\n")
    };
    Ok(format!(
        "set -e\n{pull}podman tag {image} {IMAGE_REPO}:latest\nsystemctl --user restart {API_UNIT}"
    ))
}
