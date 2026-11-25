use std::io::Read;
use std::path::{Path, PathBuf};

use crate::domain::errors::DomainError;
use crate::ports::outbound::random::RandomPort;

/// Reads the operating system's entropy file (`/dev/urandom`). A missing or
/// short source is an error: nothing weaker ever replaces it.
pub struct OsRandom {
    source: PathBuf,
}

impl OsRandom {
    pub fn system() -> Self {
        Self::from_source("/dev/urandom")
    }

    pub fn from_source(source: impl AsRef<Path>) -> Self {
        Self {
            source: source.as_ref().to_path_buf(),
        }
    }
}

impl RandomPort for OsRandom {
    fn bytes(&self, count: usize) -> Result<Vec<u8>, DomainError> {
        let mut buffer = vec![0u8; count];
        std::fs::File::open(&self.source)
            .and_then(|mut f| f.read_exact(&mut buffer))
            .map_err(|e| DomainError::ExecutionFailed {
                reason: format!("cannot read random bytes from {}", self.source.display()),
                details: Some(e.to_string()),
            })?;
        Ok(buffer)
    }
}
