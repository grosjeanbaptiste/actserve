use async_trait::async_trait;

use crate::domain::{errors::DomainError, fleet::FleetHost};

/// Lists the remote hosts to administer. The inventory itself is kept by actparc.
#[async_trait]
pub trait ServerInventoryPort: Send + Sync {
    async fn hosts(&self) -> Result<Vec<FleetHost>, DomainError>;
}
