//! The Caddy-only deploy: the commands run on the server.
use actserve::domain::caddy::{backup_command, reload_script, write_command};

#[test]
fn the_backup_copies_the_live_caddyfile_beside_itself() {
    assert_eq!(
        backup_command(),
        "cp '/opt/acteble/api/Caddyfile' '/opt/acteble/api/Caddyfile.bak'"
    );
}

#[test]
fn the_new_file_is_written_aside_then_moved_into_place() {
    assert_eq!(
        write_command(),
        "cat > '/opt/acteble/api/Caddyfile.new' && mv -f '/opt/acteble/api/Caddyfile.new' '/opt/acteble/api/Caddyfile'"
    );
}

#[test]
fn the_reload_uses_the_tmpfs_env_and_restores_the_backup_when_caddy_refuses() {
    let s = reload_script();
    assert!(s.starts_with("cd '/opt/acteble/api'\n"), "{s}");
    assert!(s.contains(
        "docker compose -f docker-compose.yml -f docker-compose.prod.yml -f docker-compose.ghcr.yml --env-file /dev/shm/acteble-api.env exec -T caddy caddy reload --config /etc/caddy/Caddyfile"
    ), "{s}");
    assert!(s.contains("mv '/opt/acteble/api/Caddyfile.bak' '/opt/acteble/api/Caddyfile'"));
    assert!(s.trim_end().ends_with("exit 1\nfi"), "{s}");
}
