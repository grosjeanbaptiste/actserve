//! The edge backup: the certificates of the running edge, as a gzip tar.

/// Under `$HOME`.
pub const DEFAULT_DIR: &str = "Backups/acteble-edge";

/// Runs on the server: finds the running Caddy container of the edge project
/// (failing when there is none), reads its data volume read-only and writes a
/// gzip tar of it to stdout.
pub const BACKUP_SCRIPT: &str = r#"c=$(docker ps -q --filter label=com.docker.compose.project=acteble_edge --filter label=com.docker.compose.service=caddy) && [ -n "$c" ] && docker run --rm --volumes-from "$c":ro --entrypoint tar "$(docker inspect -f "{{.Config.Image}}" "$c")" czf - -C /data ."#;

pub fn archive_name(stamp: &str) -> String {
    format!("acteble-edge-certs-{stamp}.tar.gz")
}
