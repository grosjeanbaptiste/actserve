//! The edge backup: archive name and the remote command.
use actserve::domain::backup::{BACKUP_SCRIPT, DEFAULT_DIR, archive_name};

#[test]
fn the_archive_name_carries_the_stamp() {
    assert_eq!(
        archive_name("20261124T210000"),
        "acteble-edge-certs-20261124T210000.tar.gz"
    );
}

#[test]
fn the_default_directory_is_under_the_home_backups() {
    assert_eq!(DEFAULT_DIR, "Backups/acteble-edge");
}

#[test]
fn the_script_finds_the_caddy_quadlet_container_and_fails_without_it() {
    assert!(BACKUP_SCRIPT.starts_with(
        "c=$(podman ps -q --filter name=^systemd-acteble_edge-caddy$) && [ -n \"$c\" ] && "
    ));
}

#[test]
fn the_script_asks_podman_for_the_image_of_that_container() {
    assert!(BACKUP_SCRIPT.contains(r#""$(podman inspect -f '{{.Config.Image}}' "$c")""#));
}

#[test]
fn the_script_has_no_docker_command_or_compose_label() {
    assert!(!BACKUP_SCRIPT.contains("docker"));
    assert!(!BACKUP_SCRIPT.contains("com.docker"));
}

#[test]
fn the_script_reads_the_volume_read_only_and_streams_a_gzip_tar_to_stdout() {
    assert!(BACKUP_SCRIPT.contains(r#"--volumes-from "$c":ro"#));
    assert!(BACKUP_SCRIPT.contains("--entrypoint tar"));
    assert!(BACKUP_SCRIPT.ends_with("czf - -C /data ."));
    assert!(BACKUP_SCRIPT.contains("podman run --rm"));
}
