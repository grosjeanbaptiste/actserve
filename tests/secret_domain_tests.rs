//! A secret value never prints; names and tmpfs paths are checked before they reach a shell.
use actserve::domain::observe::{EDGE_ENV_PATH, EDGE_SECRET, edge_status_command};
use actserve::domain::secret::{SecretValue, valid_secret_name, valid_tmpfs_path};

#[test]
fn a_secret_value_is_redacted_in_debug_and_display() {
    let s = SecretValue::new("hunter2".into());
    assert_eq!(format!("{s:?}"), "SecretValue(***)");
    assert_eq!(format!("{s}"), "***");
    assert_eq!(s.expose(), "hunter2");
}

#[test]
fn secret_names_are_lowercase_words_with_dashes() {
    for good in ["acteble-edge-prod", "ghcr-read-packages", "a1"] {
        assert!(valid_secret_name(good), "{good}");
    }
    for bad in ["", "-a", "A", "a b", "a;b", "a/b", "a'b", "$(x)"] {
        assert!(!valid_secret_name(bad), "{bad:?}");
    }
}

#[test]
fn a_tmpfs_path_is_a_plain_file_under_dev_shm() {
    assert!(valid_tmpfs_path("/dev/shm/acteble-edge.env"));
    for bad in [
        "",
        "/dev/shm/",
        "/dev/shm",
        "/tmp/x",
        "/dev/shm/../etc/passwd",
        "/dev/shm/a/b",
        "/dev/shm/a b",
        "/dev/shm/a'b",
        "/dev/shm/$x",
        "/dev/shm/-x",
        "dev/shm/x",
    ] {
        assert!(!valid_tmpfs_path(bad), "{bad:?}");
    }
}

#[test]
fn edge_status_reads_the_edge_with_its_env_file() {
    assert_eq!(EDGE_SECRET, "acteble-edge-prod");
    assert_eq!(EDGE_ENV_PATH, "/dev/shm/acteble-edge.env");
    assert_eq!(
        edge_status_command(),
        "cd '/opt/acteble/edge' && ./edge status --runtime podman --env-file /dev/shm/acteble-edge.env"
    );
}
