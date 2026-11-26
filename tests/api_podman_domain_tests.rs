//! The API stack as rootless Podman Quadlet units: files, secrets, settings and the commands run on the server.
use actserve::domain::api_podman::{
    API_SECRETS, API_SETTINGS, API_UNIT_DIR, API_UNIT_FILES, missing_env_keys, prepare_command,
    rollback_script, start_script,
};
use actserve::domain::deploy::API_ENV_PATH;

mod common;

#[test]
fn the_units_are_the_ten_files_of_the_api_quadlet_directory() {
    assert_eq!(API_UNIT_DIR, ".config/containers/systemd");
    assert_eq!(API_UNIT_FILES.len(), 10);
    assert!(
        API_UNIT_FILES
            .iter()
            .all(|f| f.starts_with("server/quadlet/acteble_api"))
    );
    for kind in [".network", ".volume", ".container"] {
        assert!(API_UNIT_FILES.iter().any(|f| f.ends_with(kind)), "{kind}");
    }
}

#[test]
fn every_secret_the_api_unit_reads_is_created_and_no_name_repeats() {
    let names: Vec<_> = API_SECRETS.iter().map(|(_, n)| *n).collect();
    for want in [
        "acteble_api_jwt_secret",
        "acteble_api_postgres_user",
        "acteble_api_postgres_password",
        "acteble_api_minio_root_user",
        "acteble_api_minio_root_password",
        "acteble_api_turnstile_secret",
        "acteble_api_livekit_egress_secret",
    ] {
        assert!(names.contains(&want), "{want}");
    }
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), API_SECRETS.len());
    assert!(names.iter().all(|n| n.starts_with("acteble_api_")));
}

#[test]
fn a_full_env_lacks_nothing_and_each_missing_key_is_named() {
    assert!(missing_env_keys(&common::full_api_env()).is_empty());
    let env = common::full_api_env().replace("TURNSTILE_SECRET=v\n", "");
    assert_eq!(missing_env_keys(&env), ["TURNSTILE_SECRET"]);
    let env = common::full_api_env().replace("LIVEKIT_URL=v\n", "");
    assert_eq!(missing_env_keys(&env), ["LIVEKIT_URL"]);
    assert_eq!(
        missing_env_keys("").len(),
        common::full_api_env().lines().count()
    );
}

#[test]
fn an_empty_or_commented_key_is_missing_and_the_last_value_wins() {
    let env = common::full_api_env().replace("TURNSTILE_SECRET=v\n", "TURNSTILE_SECRET=\n");
    assert_eq!(missing_env_keys(&env), ["TURNSTILE_SECRET"]);
    let env = common::full_api_env().replace("TURNSTILE_SECRET=v\n", "#TURNSTILE_SECRET=v\n");
    assert_eq!(missing_env_keys(&env), ["TURNSTILE_SECRET"]);
    let env = common::full_api_env().replace(
        "TURNSTILE_SECRET=v\n",
        "TURNSTILE_SECRET=v\nTURNSTILE_SECRET=\n",
    );
    assert_eq!(missing_env_keys(&env), ["TURNSTILE_SECRET"]);
    let env = common::full_api_env().replace(
        "TURNSTILE_SECRET=v\n",
        "TURNSTILE_SECRET=\nTURNSTILE_SECRET=v\n",
    );
    assert!(missing_env_keys(&env).is_empty());
    let env = common::full_api_env().replace("TURNSTILE_SECRET=v\n", "TURNSTILE_SECRET=''\n");
    assert_eq!(missing_env_keys(&env), ["TURNSTILE_SECRET"]);
}

#[test]
fn only_a_matching_pair_of_quotes_is_removed() {
    let with = |v: &str| {
        common::full_api_env().replace("TURNSTILE_SECRET=v\n", &format!("TURNSTILE_SECRET={v}\n"))
    };
    assert!(missing_env_keys(&with("\"\"")).contains(&"TURNSTILE_SECRET"));
    assert!(
        missing_env_keys(&with("'\"")).is_empty(),
        "mismatched quotes are a value"
    );
    assert!(
        missing_env_keys(&with("'")).is_empty(),
        "a lone quote is a value"
    );
}

#[test]
fn a_setting_with_a_default_is_not_required() {
    let optional: Vec<_> = API_SETTINGS.iter().filter(|s| s.2.is_some()).collect();
    assert!(!optional.is_empty());
    for s in optional {
        assert!(
            !missing_env_keys(&common::full_api_env()).contains(&s.0),
            "{}",
            s.0
        );
    }
}

#[test]
fn prepare_makes_the_stack_directory_and_the_user_unit_directory() {
    assert_eq!(
        prepare_command(),
        "mkdir -p '/opt/acteble/api' \"$HOME/.config/containers/systemd\""
    );
}

#[test]
fn the_start_script_pulls_keeps_previous_creates_secrets_then_restarts_the_api() {
    let s = start_script();
    let at = |needle: &str| s.find(needle).unwrap_or_else(|| panic!("{needle}\n{s}"));
    let tag = "podman tag ghcr.io/acteble/acteble_api:latest ghcr.io/acteble/acteble_api:previous";
    let (t, p, c, w, d, r) = (
        at(tag),
        at("podman pull ghcr.io/acteble/acteble_api:latest"),
        at("\nsecret APP_AUTH__JWT_SECRET "),
        at("/opt/acteble/api/api.env"),
        at("systemctl --user daemon-reload"),
        at("systemctl --user restart acteble_api-api.service"),
    );
    assert!(t < p && p < c && c < w && w < d && d < r, "{s}");
    assert!(s.starts_with("set -e\n"));
    assert!(s.contains(API_ENV_PATH));
}

#[test]
fn the_start_script_wires_every_secret_and_setting_and_starts_the_backing_units() {
    let s = start_script();
    for (key, name) in API_SECRETS {
        assert!(s.contains(&format!("secret {key} {name}\n")), "{key}");
    }
    for (key, var, default) in API_SETTINGS {
        let d = default.unwrap_or("");
        assert!(s.contains(&format!("setting {key} {var} '{d}'\n")), "{key}");
    }
    for unit in ["postgres", "redis", "minio", "pg-backup"] {
        assert!(s.contains(&format!("acteble_api-{unit}.service")), "{unit}");
    }
    assert!(s.contains("systemctl --user start "));
}

#[test]
fn the_start_script_never_sources_the_env_and_holds_no_value() {
    let s = start_script();
    assert!(!s.contains(". /dev/shm") && !s.contains("set -a"), "{s}");
    assert!(!s.contains("docker"));
    assert!(s.contains("(umask 077;"), "api.env must be private");
}

#[test]
fn a_rollback_to_previous_retags_and_restarts_without_pulling() {
    let s = rollback_script("previous").unwrap();
    assert_eq!(
        s,
        "set -e\n\
         podman tag ghcr.io/acteble/acteble_api:previous ghcr.io/acteble/acteble_api:latest\n\
         systemctl --user restart acteble_api-api.service"
    );
}

#[test]
fn a_rollback_to_a_tag_pulls_it_first() {
    let s = rollback_script("sha-abc123").unwrap();
    assert_eq!(
        s,
        "set -e\n\
         podman pull ghcr.io/acteble/acteble_api:sha-abc123\n\
         podman tag ghcr.io/acteble/acteble_api:sha-abc123 ghcr.io/acteble/acteble_api:latest\n\
         systemctl --user restart acteble_api-api.service"
    );
}

#[test]
fn a_tag_that_could_break_the_quoting_is_refused() {
    for bad in ["", "a b", "a'b", "a;b", "$(x)", "-x", "a/b", ":latest"] {
        assert!(rollback_script(bad).is_err(), "{bad:?}");
    }
}
