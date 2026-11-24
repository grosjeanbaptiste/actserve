//! Runs a command on a server with a secret as an env file on its tmpfs.
use crate::domain::{
    errors::DomainError,
    secret::{SecretValue, valid_secret_name, valid_tmpfs_path},
};
use crate::ports::outbound::{
    remote_shell::{RemoteOutput, RemoteShellPort},
    secret_store::SecretStorePort,
};

fn failure(reason: String) -> DomainError {
    DomainError::ExecutionFailed {
        reason,
        details: None,
    }
}

/// Reads `secret` from the store, then runs `command` with it as an env file
/// (see `with_remote_env_value`). A name that is not a secret name is refused
/// before the store is asked.
pub async fn with_remote_env(
    shell: &dyn RemoteShellPort,
    secrets: &dyn SecretStorePort,
    target: &str,
    secret: &str,
    path: &str,
    command: &str,
) -> Result<RemoteOutput, DomainError> {
    if !valid_secret_name(secret) {
        return Err(DomainError::ValidationError(format!(
            "invalid secret name {secret:?}"
        )));
    }
    if !valid_tmpfs_path(path) {
        return Err(DomainError::ValidationError(format!(
            "invalid tmpfs path {path:?}"
        )));
    }
    let value = secrets.get(secret).await?;
    with_remote_env_value(shell, target, &value, path, command, None).await
}

/// Puts `value` (sent over stdin, never in an argument) in `path` on the
/// server's tmpfs with mode 0600, runs `command` (fed `command_stdin`), then
/// removes the file whether `command` succeeded or not. The command's answer is
/// returned; a file that was not written or not removed is an error.
pub async fn with_remote_env_value(
    shell: &dyn RemoteShellPort,
    target: &str,
    value: &SecretValue,
    path: &str,
    command: &str,
    command_stdin: Option<&str>,
) -> Result<RemoteOutput, DomainError> {
    if !valid_tmpfs_path(path) {
        return Err(DomainError::ValidationError(format!(
            "invalid tmpfs path {path:?}"
        )));
    }
    let write = format!("umask 077 && cat > '{path}'");
    let written = shell.run(target, &write, Some(value.expose())).await;
    let ran = match &written {
        Ok(w) if w.code == Some(0) => Some(shell.run(target, command, command_stdin).await),
        _ => None,
    };
    let removed = shell.run(target, &format!("rm -f '{path}'"), None).await?;
    if removed.code != Some(0) {
        return Err(failure(format!(
            "could not remove {path} from the server: {}",
            removed.stderr.trim()
        )));
    }
    match (written?, ran) {
        (w, None) => Err(failure(format!(
            "could not write {path} on the server: {}",
            w.stderr.trim()
        ))),
        (_, Some(out)) => out,
    }
}
