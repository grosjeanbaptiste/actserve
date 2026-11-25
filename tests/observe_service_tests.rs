//! `ObserveService`: ps and logs of a stack, only on the expected host.
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::application::observe_service::{ObserveRun, ObserveService, exit_code, render};
use actserve::domain::{
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    observe::Stack,
};
use actserve::ports::outbound::{
    hostname_probe::HostnameProbePort,
    remote_shell::{RemoteOutput, RemoteShellPort},
    remote_terminal::RemoteTerminalPort,
    server_inventory::ServerInventoryPort,
};

struct Inventory;
#[async_trait]
impl ServerInventoryPort for Inventory {
    async fn hosts(&self) -> Result<Vec<FleetHost>, DomainError> {
        Ok(vec![FleetHost::new("a", "ta", "h1").unwrap()])
    }
}
struct Probe(Result<&'static str, &'static str>);
#[async_trait]
impl HostnameProbePort for Probe {
    async fn hostname(&self, _: &str) -> Result<String, DomainError> {
        self.0
            .map(String::from)
            .map_err(|m| DomainError::ExecutionFailed {
                reason: "cannot reach".into(),
                details: Some(m.into()),
            })
    }
}
#[derive(Default)]
struct Calls(Mutex<Vec<String>>);
struct Shell(Arc<Calls>, Option<i32>);
#[async_trait]
impl RemoteShellPort for Shell {
    async fn run(&self, t: &str, c: &str, i: Option<&str>) -> Result<RemoteOutput, DomainError> {
        assert!(i.is_none());
        self.0.0.lock().unwrap().push(format!("shell {t} {c}"));
        Ok(RemoteOutput {
            code: self.1,
            stdout: "NAME\napi\n".into(),
            stderr: "warn\n".into(),
        })
    }
}
struct Terminal(Arc<Calls>, Option<i32>);
#[async_trait]
impl RemoteTerminalPort for Terminal {
    async fn attach(&self, t: &str, c: &str) -> Result<Option<i32>, DomainError> {
        self.0.0.lock().unwrap().push(format!("tty {t} {c}"));
        Ok(self.1)
    }
}

fn service(probe: Probe, shell: Option<i32>, tty: Option<i32>) -> (ObserveService, Arc<Calls>) {
    let calls = Arc::new(Calls::default());
    let s = ObserveService::new(
        Arc::new(Inventory),
        Arc::new(probe),
        Arc::new(Shell(calls.clone(), shell)),
        Arc::new(Terminal(calls.clone(), tty)),
    );
    (s, calls)
}

#[tokio::test]
async fn ps_runs_podman_ps_and_returns_the_answer() {
    let (s, calls) = service(Probe(Ok("h1")), Some(0), None);
    let run = s.ps("a", Stack::Cdn).await.unwrap();
    assert_eq!(
        calls.0.lock().unwrap().as_slice(),
        ["shell ta podman ps --filter name=systemd-acteble_cdn"]
    );
    assert_eq!(exit_code(&run), 0);
    assert_eq!(render("a", &run), "NAME\napi\n");
}

#[tokio::test]
async fn a_failing_ps_exits_with_its_code() {
    let (s, _) = service(Probe(Ok("h1")), Some(7), None);
    assert_eq!(exit_code(&s.ps("a", Stack::Api).await.unwrap()), 7);
}

#[tokio::test]
async fn a_signalled_ps_exits_1() {
    let (s, _) = service(Probe(Ok("h1")), None, None);
    assert_eq!(exit_code(&s.ps("a", Stack::Api).await.unwrap()), 1);
}

#[tokio::test]
async fn logs_attach_a_terminal_with_the_default_service() {
    let (s, calls) = service(Probe(Ok("h1")), None, Some(0));
    let run = s.logs("a", Stack::Cdn, None).await.unwrap();
    assert_eq!(
        calls.0.lock().unwrap().as_slice(),
        [
            "tty ta journalctl --user -u acteble_cdn-checkpoint.service -f -n 100"
        ]
    );
    assert_eq!(exit_code(&run), 0);
    assert_eq!(render("a", &run), "");
}

#[tokio::test]
async fn logs_of_a_named_service_exit_with_the_terminal_code() {
    let (s, calls) = service(Probe(Ok("h1")), None, Some(130));
    let run = s.logs("a", Stack::Api, Some("db")).await.unwrap();
    assert!(calls.0.lock().unwrap()[0].ends_with("acteble_api-db.service -f -n 100"));
    assert_eq!(exit_code(&run), 130);
}

#[tokio::test]
async fn a_bad_service_name_runs_nothing() {
    let (s, calls) = service(Probe(Ok("h1")), None, Some(0));
    let e = s.logs("a", Stack::Api, Some("x'y")).await.unwrap_err();
    assert!(matches!(e, DomainError::ValidationError(_)));
    assert!(calls.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_unknown_server_is_not_found() {
    let (s, _) = service(Probe(Ok("h1")), Some(0), None);
    assert!(matches!(
        s.ps("zz", Stack::Api).await.unwrap_err(),
        DomainError::NotFound { .. }
    ));
}

#[tokio::test]
async fn the_wrong_host_is_refused_and_nothing_runs() {
    let (s, calls) = service(Probe(Ok("other")), Some(0), Some(0));
    for run in [
        s.ps("a", Stack::Api).await.unwrap(),
        s.logs("a", Stack::Api, None).await.unwrap(),
    ] {
        assert!(matches!(
            run,
            ObserveRun::Refused(HostStatus::WrongHost { .. })
        ));
        assert_eq!(exit_code(&run), 3);
        assert_eq!(
            render("a", &run),
            "a: wrong-host (answered other, expected h1)\n"
        );
    }
    assert!(calls.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_unreachable_host_is_refused_with_its_reason() {
    let (s, _) = service(Probe(Err("timeout")), Some(0), Some(0));
    let run = s.ps("a", Stack::Api).await.unwrap();
    assert_eq!(exit_code(&run), 3);
    assert_eq!(
        render("a", &run),
        "a: unreachable (cannot reach: timeout)\n"
    );
}
