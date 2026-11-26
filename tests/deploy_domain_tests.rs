//! The commands of the API deploy and rollback, and the ghcr login that wraps them.
use actserve::domain::deploy::{
    API_COMPOSE_FILES, API_ENV_PATH, API_SECRET, GHCR_SECRET, deploy_script, ghcr_wrapped,
    rollback_script, valid_image_tag,
};
use actserve::domain::secret::{valid_secret_name, valid_tmpfs_path};

const BASE: &str = "-f docker-compose.yml -f docker-compose.prod.yml -f docker-compose.ghcr.yml";

#[test]
fn the_names_are_valid_secret_and_tmpfs_names() {
    assert!(valid_secret_name(API_SECRET) && valid_secret_name(GHCR_SECRET));
    assert!(valid_tmpfs_path(API_ENV_PATH));
    assert_eq!(API_SECRET, "acteble-api-prod");
    assert_eq!(GHCR_SECRET, "ghcr-read-packages");
}

#[test]
fn the_files_sent_are_the_compose_files_and_the_caddyfile() {
    assert_eq!(
        API_COMPOSE_FILES,
        [
            "docker-compose.yml",
            "docker-compose.prod.yml",
            "docker-compose.ghcr.yml",
            "Caddyfile"
        ]
    );
}

#[test]
fn ghcr_login_wraps_a_script_and_always_logs_out() {
    assert_eq!(
        ghcr_wrapped("echo hi"),
        "docker login ghcr.io -u 'grosjeanbaptiste' --password-stdin >/dev/null || exit 1\n\
         (\necho hi\n)\nrc=$?\ndocker logout ghcr.io >/dev/null 2>&1\nexit $rc"
    );
}

#[test]
fn the_deploy_keeps_the_old_image_pulls_restarts_and_prunes() {
    let s = deploy_script();
    let env = format!("--env-file {API_ENV_PATH}");
    assert_eq!(
        s,
        format!(
            "cd '/opt/acteble/api'\n\
             docker image tag 'ghcr.io/acteble/acteble_api:latest' 'ghcr.io/acteble/acteble_api:previous' || true\n\
             docker compose {BASE} {env} pull api\n\
             docker compose {BASE} {env} up -d --no-build --remove-orphans\n\
             docker image prune -f"
        )
    );
}

#[test]
fn rolling_back_to_previous_restarts_without_pulling() {
    let s = rollback_script("previous").unwrap();
    let env = format!("--env-file {API_ENV_PATH}");
    assert_eq!(
        s,
        format!(
            "cd '/opt/acteble/api'\n\
             API_IMAGE='ghcr.io/acteble/acteble_api:previous' docker compose {BASE} {env} up -d --no-build"
        )
    );
}

#[test]
fn rolling_back_to_a_tag_pulls_it_first() {
    let s = rollback_script("sha-abc123").unwrap();
    let env = format!("--env-file {API_ENV_PATH}");
    assert!(s.contains(&format!(
        "API_IMAGE='ghcr.io/acteble/acteble_api:sha-abc123' docker compose {BASE} {env} pull api\n"
    )));
    assert!(s.ends_with("up -d --no-build"));
}

#[test]
fn an_image_tag_that_could_break_the_quoting_is_refused() {
    for bad in ["", "a b", "a'b", "a;b", "$(x)", "-x", "a/b", ":latest"] {
        assert!(!valid_image_tag(bad), "{bad:?}");
        assert!(rollback_script(bad).is_err(), "{bad:?}");
    }
    for good in ["previous", "latest", "sha-0a1b2c", "v1.2.3", "A_b-c.d"] {
        assert!(valid_image_tag(good), "{good:?}");
    }
}
