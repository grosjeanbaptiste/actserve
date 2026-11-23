use async_trait::async_trait;

use crate::domain::{errors::DomainError, stacks::DeployOutcome};
use crate::ports::outbound::stack_deploy::StackDeployPort;

/// Stand-in until the deploy is ported (ROADMAP slice 4): starts nothing and
/// says so, so a bootstrap that needs a start fails loudly instead of
/// claiming a server it did not bring up.
pub struct UnportedStackDeploy;

#[async_trait]
impl StackDeployPort for UnportedStackDeploy {
    async fn start(&self, _ssh_target: &str, stack: &str) -> Result<DeployOutcome, DomainError> {
        Ok(DeployOutcome::Unavailable(format!(
            "starting {stack} is not ported to actserve yet (ROADMAP slice 4)"
        )))
    }
}
