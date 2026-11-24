//! The CDN deploy: names and the commands run on the server.
use actserve::domain::cdn::{
    CDN_ENV_PATH, CDN_SECRET, geo_script, network_command, prepare_command, start_script,
};

#[test]
fn the_names_are_the_ones_of_the_bash_deploy() {
    assert_eq!(CDN_SECRET, "acteble-cdn-prod");
    assert_eq!(CDN_ENV_PATH, "/dev/shm/acteble-cdn.env");
}

#[test]
fn the_directory_is_created_idempotently() {
    assert_eq!(prepare_command(), "mkdir -p '/opt/acteble/cdn'");
}

#[test]
fn the_shared_network_is_created_only_when_absent_and_a_failed_create_is_not_hidden() {
    assert_eq!(
        network_command(),
        "docker network inspect acteble_edge > /dev/null 2>&1 || docker network create acteble_edge > /dev/null"
    );
}

#[test]
fn the_start_pulls_then_brings_the_stack_up_with_the_tmpfs_env() {
    assert_eq!(
        start_script(),
        "cd '/opt/acteble/cdn'\n\
         docker compose --env-file /dev/shm/acteble-cdn.env pull && docker compose --env-file /dev/shm/acteble-cdn.env up -d --remove-orphans"
    );
}

#[test]
fn the_geo_prefix_is_opened_by_a_throwaway_read_only_client_with_retries() {
    let s = geo_script();
    assert!(
        s.starts_with("set -a && . /dev/shm/acteble-cdn.env && set +a\n"),
        "{s}"
    );
    assert!(
        s.contains("MC_HOST_local=\"http://$MINIO_ROOT_USER:$MINIO_ROOT_PASSWORD@cdn-minio:9000\""),
        "{s}"
    );
    assert!(s.contains("--network acteble_cdn"), "{s}");
    assert!(s.contains("--cap-drop ALL --read-only --tmpfs /tmp"), "{s}");
    assert!(s.contains("cgr.dev/chainguard/minio-client@sha256:10388d580a724f356b94c9b2631b5a179039cabe8c76e9b37d86ed007d569b05"), "{s}");
    assert!(
        s.contains("anonymous set download local/acteble-files/geo"),
        "{s}"
    );
    assert!(s.contains("for i in 1 2 3 4 5 6 7 8 9 10"), "{s}");
    assert!(s.trim_end().ends_with("exit 1"), "{s}");
}
