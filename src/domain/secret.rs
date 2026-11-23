//! A secret value that cannot leak through a log line, and the names and paths that may carry one.
use std::fmt;

/// The text of a secret. Printing it, in `Debug` or `Display`, shows `***`.
pub struct SecretValue(String);

impl SecretValue {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// The only way to read it; the caller decides where it goes.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue(***)")
    }
}

impl fmt::Display for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// A secret store name: lowercase letters, digits and `-`, never leading `-`.
pub fn valid_secret_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A file directly under `/dev/shm` (memory only), with a name that is safe
/// inside single quotes and cannot be read as an option.
pub fn valid_tmpfs_path(path: &str) -> bool {
    let Some(file) = path.strip_prefix("/dev/shm/") else {
        return false;
    };
    !file.is_empty()
        && !file.starts_with('-')
        && file != "."
        && file != ".."
        && file
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}
