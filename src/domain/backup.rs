//! The edge backup: the certificates of the running edge, as a gzip tar.

/// Under `$HOME`.
pub const DEFAULT_DIR: &str = "Backups/acteble-edge";

/// Runs on the server as the rootless Podman user: finds the running Caddy container of
/// the edge (the Quadlet unit `acteble_edge-caddy` names it `systemd-acteble_edge-caddy`;
/// fails when there is none), reads its data volume read-only and writes a gzip tar of it
/// to stdout.
pub const BACKUP_SCRIPT: &str = r#"c=$(podman ps -q --filter name=^systemd-acteble_edge-caddy$) && [ -n "$c" ] && podman run --rm --volumes-from "$c":ro --entrypoint tar "$(podman inspect -f '{{.Config.Image}}' "$c")" czf - -C /data ."#;

pub fn archive_name(stamp: &str) -> String {
    format!("acteble-edge-certs-{stamp}.tar.gz")
}
