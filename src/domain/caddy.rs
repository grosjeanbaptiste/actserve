//! The Caddy-only deploy of the API server: the commands run on the server.
use crate::domain::{deploy::compose, observe::Stack};

fn caddyfile() -> String {
    format!("{}/Caddyfile", Stack::Api.dir())
}

/// Keeps the live Caddyfile aside so a refused config can be undone on disk.
pub fn backup_command() -> String {
    let live = caddyfile();
    format!("cp '{live}' '{live}.bak'")
}

/// Reads the new Caddyfile on stdin; it replaces the live one only once fully written.
pub fn write_command() -> String {
    let live = caddyfile();
    format!("cat > '{live}.new' && mv -f '{live}.new' '{live}'")
}

/// Reloads Caddy; when it refuses the config, restores the backup and fails.
pub fn reload_script() -> String {
    let live = caddyfile();
    format!(
        "cd '{dir}'\n\
         if {reload}; then\n\
         echo '==> Caddy reloaded'\n\
         else\n\
         echo '!! reload failed: restoring previous Caddyfile' >&2\n\
         mv '{live}.bak' '{live}'\n\
         exit 1\n\
         fi",
        dir = Stack::Api.dir(),
        reload = compose("exec -T caddy caddy reload --config /etc/caddy/Caddyfile"),
    )
}
