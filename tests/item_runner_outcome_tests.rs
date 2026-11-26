//! How the answer of a remote command becomes the outcome of a report item.
use actserve::application::item_runner::outcome;
use actserve::domain::{errors::DomainError, harden::ItemOutcome};
use actserve::ports::outbound::remote_shell::RemoteOutput;

fn out(code: Option<i32>) -> RemoteOutput {
    RemoteOutput {
        code,
        stdout: String::new(),
        stderr: " boom \n".into(),
    }
}

#[test]
fn exit_zero_is_applied() {
    assert_eq!(outcome(Ok(out(Some(0)))), ItemOutcome::Applied);
}

#[test]
fn another_exit_is_a_failure_with_its_code_and_stderr() {
    assert_eq!(
        outcome(Ok(out(Some(5)))),
        ItemOutcome::Failed("exit 5: boom".into())
    );
}

#[test]
fn a_signal_is_a_failure_not_a_success() {
    assert_eq!(
        outcome(Ok(out(None))),
        ItemOutcome::Failed("exit signal: boom".into())
    );
}

#[test]
fn an_error_is_a_failure_with_its_details() {
    let e = DomainError::ExecutionFailed {
        reason: "ssh".into(),
        details: Some("refused".into()),
    };
    assert_eq!(outcome(Err(e)), ItemOutcome::Failed("ssh: refused".into()));
}
