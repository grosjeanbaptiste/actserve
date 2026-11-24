//! The real `StackDeployPort` for the CLI: edge and cdn deploy services with
//! their local directories (`$ACTEBLE_EDGE_DIR`, `$ACTEBLE_CDN_DIR`).
use std::sync::Arc;

use crate::application::stack_deployer::ServiceStackDeploy;
use crate::{cdn_cli, edge_cli};

pub(crate) fn stack_deploy() -> Result<Arc<ServiceStackDeploy>, Box<dyn std::error::Error>> {
    Ok(Arc::new(ServiceStackDeploy::new(
        edge_cli::service()?,
        cdn_cli::service()?,
        &edge_cli::edge_dir()?,
        &cdn_cli::cdn_dir()?,
    )))
}
