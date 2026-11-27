//! The edge deploy: what is copied and the commands run on the server (Podman, Quadlet).
use actserve::domain::edge_deploy::{
    EDGE_FILES, EDGE_IMAGE, binary_script, prepare_command, start_command, units_command,
};

#[test]
fn the_edge_files_are_the_caddyfiles_the_sites_and_the_units() {
    assert_eq!(
        EDGE_FILES,
        ["Caddyfile", "Caddyfile.socket", "sites", "quadlet"]
    );
    assert_eq!(EDGE_IMAGE, "ghcr.io/acteble/acteble_edge:latest");
}

#[test]
fn the_directories_are_created_idempotently() {
    assert_eq!(
        prepare_command(),
        "mkdir -p '/opt/acteble/edge' \"$HOME/.config/containers/systemd\" \"$HOME/.config/systemd/user\""
    );
}

#[test]
fn the_quadlet_units_go_where_quadlet_reads_them_and_the_socket_to_the_systemd_user_dir() {
    assert_eq!(
        units_command(),
        "cp '/opt/acteble/edge/quadlet'/*.container '/opt/acteble/edge/quadlet'/*.network '/opt/acteble/edge/quadlet'/*.volume \"$HOME/.config/containers/systemd/\" && cp '/opt/acteble/edge/quadlet'/*.socket \"$HOME/.config/systemd/user/\""
    );
}

#[test]
fn the_binary_is_taken_out_of_the_image_and_the_temporary_container_is_always_removed() {
    assert_eq!(
        binary_script(),
        "podman pull ghcr.io/acteble/acteble_edge:latest && id=$(podman create ghcr.io/acteble/acteble_edge:latest /edge) && podman cp $id:/edge /opt/acteble/edge/edge; rc=$?; podman rm $id >/dev/null 2>&1; exit $rc"
    );
}

#[test]
fn the_start_keeps_the_env_for_the_unit_then_runs_the_edge_under_podman() {
    assert_eq!(
        start_command(),
        "install -m 600 /dev/shm/acteble-edge.env '/opt/acteble/edge/.env' && cd '/opt/acteble/edge' && ./edge up --runtime podman --env-file /opt/acteble/edge/.env"
    );
}

#[test]
fn no_command_names_docker() {
    for c in [
        prepare_command(),
        units_command(),
        binary_script(),
        start_command(),
    ] {
        assert!(!c.contains("docker") && !c.contains("compose"), "{c}");
    }
}
