use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::application::{
    cdn_deploy_service::CdnDeployService, edge_deploy_service::EdgeDeployService,
};
use crate::domain::{
    errors::DomainError,
    harden::{ItemOutcome, ItemReport},
    stacks::DeployOutcome,
};
use crate::ports::outbound::stack_deploy::StackDeployPort;

/// The real `StackDeployPort`: `"edge"` is the edge deploy, `"cdn"` the cdn
/// deploy. The caller (`StacksService`) already checked the host, so the
/// services run on the bare target. Any other stack is refused by name.
pub struct ServiceStackDeploy {
    edge: EdgeDeployService,
    cdn: CdnDeployService,
    edge_dir: PathBuf,
    cdn_dir: PathBuf,
}

impl ServiceStackDeploy {
    pub fn new(
        edge: EdgeDeployService,
        cdn: CdnDeployService,
        edge_dir: &Path,
        cdn_dir: &Path,
    ) -> Self {
        Self {
            edge,
            cdn,
            edge_dir: edge_dir.to_path_buf(),
            cdn_dir: cdn_dir.to_path_buf(),
        }
    }
}

/// `Started` when every step applied; otherwise the first failed step, named.
fn outcome(reports: &[ItemReport]) -> DeployOutcome {
    for r in reports {
        if let ItemOutcome::Failed(reason) = &r.outcome {
            return DeployOutcome::Unavailable(format!("{} {}: {reason}", r.step, r.item));
        }
    }
    DeployOutcome::Started
}

#[async_trait]
impl StackDeployPort for ServiceStackDeploy {
    async fn start(&self, ssh_target: &str, stack: &str) -> Result<DeployOutcome, DomainError> {
        let ran = match stack {
            "edge" => {
                self.edge
                    .deploy_verified(ssh_target, &self.edge_dir, false)
                    .await
            }
            "cdn" => {
                self.cdn
                    .deploy_verified(ssh_target, &self.cdn_dir, false)
                    .await
            }
            other => {
                return Ok(DeployOutcome::Unavailable(format!(
                    "{other} is not started by the bootstrap: use `actserve deploy`"
                )));
            }
        };
        Ok(match ran {
            Ok(reports) => outcome(&reports),
            // Only the local file check can fail here; it is reported, not raised.
            Err(e) => DeployOutcome::Unavailable(e.to_string()),
        })
    }
}
