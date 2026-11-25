use crate::domain::errors::DomainError;

/// Cryptographically secure random bytes. An error means the source could not
/// be read; no weaker source ever stands in for it.
pub trait RandomPort: Send + Sync {
    fn bytes(&self, count: usize) -> Result<Vec<u8>, DomainError>;
}
