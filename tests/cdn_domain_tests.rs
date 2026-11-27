//! The CDN deploy: names and the commands run on the server.
use actserve::domain::cdn::{
    CDN_ENV_PATH, CDN_HEALTH_SCRIPT, CDN_SECRET, CDN_UNIT_DIR, CDN_UNIT_FILES, geo_script,
    network_command, prepare_command, start_script,
};

#[test]
fn the_names_are_the_ones_of_the_bash_deploy() {
    assert_eq!(CDN_SECRET, "acteble-cdn-prod");
    assert_eq!(CDN_ENV_PATH, "/dev/shm/acteble-cdn.env");
}

#[test]
fn the_directories_are_created_idempotently() {
    assert_eq!(
        prepare_command(),
        "mkdir -p '/opt/acteble/cdn' && mkdir -p \"$HOME/.config/containers/systemd\""
    );
}

#[test]
fn the_units_are_the_quadlet_files_of_the_cdn_repository_and_land_in_the_user_unit_dir() {
    assert_eq!(CDN_UNIT_DIR, ".config/containers/systemd");
    assert_eq!(
        CDN_UNIT_FILES,
        [
            "quadlet/acteble_cdn.network",
            "quadlet/acteble_cdn_minio.volume",
            "quadlet/acteble_cdn_geo_work.volume",
            "quadlet/acteble_cdn-minio.kube",
            "quadlet/acteble_cdn-minio.yaml",
            "quadlet/acteble_cdn-checkpoint.kube",
            "quadlet/acteble_cdn-checkpoint.yaml",
            "quadlet/acteble_cdn-publisher.container",
            "quadlet/acteble_cdn-geo-builder.container",
        ]
    );
    assert_eq!(CDN_HEALTH_SCRIPT, "quadlet/minio-health.sh");
}

#[test]
fn the_shared_network_is_started_only_when_absent_after_the_units_are_loaded() {
    assert_eq!(
        network_command(),
        "rm -f \"$HOME/.config/containers/systemd/acteble_cdn-minio.container\" \"$HOME/.config/containers/systemd/acteble_cdn-checkpoint.container\" && systemctl --user daemon-reload && { podman network exists acteble_edge || systemctl --user start acteble_edge-network.service; }"
    );
}

#[test]
fn the_start_creates_the_podman_secrets_from_the_env_then_restarts_the_units() {
    let s = start_script();
    assert!(
        s.starts_with("set -e\nset -a && . /dev/shm/acteble-cdn.env && set +a\n"),
        "{s}"
    );
    assert!(
        s.contains("podman pull ghcr.io/acteble/acteble_cdn:latest\n"),
        "{s}"
    );
    for (var, secret) in [
        ("MINIO_ROOT_USER", "acteble_cdn_minio_root_user"),
        ("MINIO_ROOT_PASSWORD", "acteble_cdn_minio_root_password"),
        ("CDN_TOKEN_SECRET", "acteble_cdn_token_secret"),
        (
            "CDN_MANIFEST_PUBLISHER_KEY_ID",
            "acteble_cdn_manifest_publisher_key_id",
        ),
    ] {
        assert!(
            s.contains(&format!("secret {secret} \"${var}\"\n")),
            "{secret}: {s}"
        );
    }
    // The `.kube` units read one `v1.Secret`, values in base64 (any byte survives YAML).
    assert!(s.contains("podman secret create --replace acteble-cdn -\n"), "{s}");
    for key in ["minio_root_user", "minio_root_password", "token_secret", "manifest_publisher_key_id"] {
        assert!(s.contains(&format!("  {key}: %s")), "{key}: {s}");
    }
    assert!(s.contains("kind: Secret"), "{s}");
    assert!(
        s.contains("[ -z \"$CDN_MANIFEST_SIGNING_KEY\" ] || secret acteble_cdn_manifest_signing_key \"$CDN_MANIFEST_SIGNING_KEY\"\n"),
        "{s}"
    );
    assert!(
        s.contains("secret() { printf %s \"$2\" | podman secret create --replace \"$1\" -; }\n"),
        "{s}"
    );
    assert!(
        s.trim_end().ends_with(
            "systemctl --user daemon-reload\nsystemctl --user restart acteble_cdn-minio.service acteble_cdn-checkpoint.service"
        ),
        "{s}"
    );
    assert!(!s.contains("docker"), "{s}");
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
    assert!(s.contains("podman run --rm --network acteble_cdn"), "{s}");
    assert!(!s.contains("docker"), "{s}");
    assert!(s.contains("--cap-drop ALL --read-only --tmpfs /tmp"), "{s}");
    assert!(s.contains("cgr.dev/chainguard/minio-client@sha256:10388d580a724f356b94c9b2631b5a179039cabe8c76e9b37d86ed007d569b05"), "{s}");
    assert!(
        s.contains("anonymous set download local/acteble-files/geo"),
        "{s}"
    );
    assert!(s.contains("for i in 1 2 3 4 5 6 7 8 9 10"), "{s}");
    assert!(s.trim_end().ends_with("exit 1"), "{s}");
}
