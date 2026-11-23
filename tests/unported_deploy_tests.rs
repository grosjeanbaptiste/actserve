use actserve::adapters::outbound::system::unported_stack_deploy::UnportedStackDeploy;
use actserve::domain::stacks::DeployOutcome;
use actserve::ports::outbound::stack_deploy::StackDeployPort;

#[tokio::test]
async fn it_never_starts_anything_and_names_the_stack_and_the_roadmap_slice() {
    for stack in ["edge", "cdn"] {
        match UnportedStackDeploy.start("t", stack).await.unwrap() {
            DeployOutcome::Unavailable(m) => {
                assert!(
                    m.contains(stack) && m.contains("not ported") && m.contains("slice 4"),
                    "{m}"
                );
            }
            DeployOutcome::Started => panic!("started {stack}"),
        }
    }
}
