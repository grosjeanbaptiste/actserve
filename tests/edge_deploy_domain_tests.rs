//! The edge deploy: what is copied and the commands run on the server.
use actserve::domain::edge_deploy::{
    EDGE_FILES, EDGE_IMAGE, binary_script, prepare_command, start_command,
};

#[test]
fn the_edge_files_are_the_three_of_the_bash_deploy() {
    assert_eq!(EDGE_FILES, ["Caddyfile", "compose.yml", "sites"]);
    assert_eq!(EDGE_IMAGE, "ghcr.io/acteble/acteble_edge:latest");
}

#[test]
fn the_directory_is_created_idempotently() {
    assert_eq!(prepare_command(), "mkdir -p '/opt/acteble/edge'");
}

#[test]
fn the_binary_is_taken_out_of_the_image_and_the_temporary_container_is_always_removed() {
    assert_eq!(
        binary_script(),
        "docker pull ghcr.io/acteble/acteble_edge:latest && id=$(docker create ghcr.io/acteble/acteble_edge:latest /edge) && docker cp $id:/edge /opt/acteble/edge/edge; rc=$?; docker rm $id >/dev/null 2>&1; exit $rc"
    );
}

#[test]
fn the_start_runs_the_edge_with_the_tmpfs_env() {
    assert_eq!(
        start_command(),
        "cd '/opt/acteble/edge' && ./edge up --env-file /dev/shm/acteble-edge.env"
    );
}
