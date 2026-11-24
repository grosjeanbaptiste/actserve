//! `EdgeBackupService`: the edge backup, every port faked.
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::application::edge_backup_service::{BackupRun, EdgeBackupService, exit_code, render};
use actserve::domain::{
    backup::BACKUP_SCRIPT,
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
};
use actserve::ports::outbound::{
    hostname_probe::HostnameProbePort, remote_download::RemoteDownloadPort,
    remote_shell::RemoteOutput, server_inventory::ServerInventoryPort,
};

struct Inventory;
#[async_trait]
impl ServerInventoryPort for Inventory {
    async fn hosts(&self) -> Result<Vec<FleetHost>, DomainError> {
        Ok(vec![FleetHost::new("a", "ta", "h1").unwrap()])
    }
}
struct Probe(&'static str);
#[async_trait]
impl HostnameProbePort for Probe {
    async fn hostname(&self, _: &str) -> Result<String, DomainError> {
        Ok(self.0.into())
    }
}
struct Download(Arc<Mutex<Vec<String>>>, Result<Option<i32>, &'static str>);
#[async_trait]
impl RemoteDownloadPort for Download {
    async fn download(&self, t: &str, c: &str, d: &Path) -> Result<RemoteOutput, DomainError> {
        self.0
            .lock()
            .unwrap()
            .push(format!("{t} | {c} | {}", d.display()));
        match &self.1 {
            Ok(code) => Ok(RemoteOutput {
                code: *code,
                stdout: String::new(),
                stderr: "no caddy".into(),
            }),
            Err(m) => Err(DomainError::ExecutionFailed {
                reason: (*m).into(),
                details: None,
            }),
        }
    }
}

fn service(
    host: &'static str,
    dl: Result<Option<i32>, &'static str>,
) -> (EdgeBackupService, Arc<Mutex<Vec<String>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let s = EdgeBackupService::new(
        Arc::new(Inventory),
        Arc::new(Probe(host)),
        Arc::new(Download(log.clone(), dl)),
    );
    (s, log)
}

#[tokio::test]
async fn a_backup_downloads_the_script_output_into_a_stamped_file() {
    let (s, log) = service("h1", Ok(Some(0)));
    let run = s
        .backup("a", Path::new("/b"), "20261124T210000")
        .await
        .unwrap();
    let dest = PathBuf::from("/b/acteble-edge-certs-20261124T210000.tar.gz");
    assert_eq!(run, BackupRun::Saved(dest.clone()));
    assert_eq!(exit_code(&run), 0);
    assert_eq!(render("a", &run), format!("==> Saved {}\n", dest.display()));
    assert_eq!(
        *log.lock().unwrap(),
        [format!("ta | {BACKUP_SCRIPT} | {}", dest.display())]
    );
}

#[tokio::test]
async fn another_host_is_refused_with_exit_3_and_nothing_is_downloaded() {
    let (s, log) = service("someone-else", Ok(Some(0)));
    let run = s.backup("a", Path::new("/b"), "t").await.unwrap();
    assert!(matches!(
        run,
        BackupRun::Refused(HostStatus::WrongHost { .. })
    ));
    assert_eq!(exit_code(&run), 3);
    assert!(render("a", &run).contains("wrong-host"));
    assert!(log.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_failing_transfer_says_nothing_was_saved_and_carries_the_remote_error() {
    for code in [Some(1), None] {
        let (s, _) = service("h1", Ok(code));
        let run = s.backup("a", Path::new("/b"), "t").await.unwrap();
        assert!(matches!(run, BackupRun::Failed(_)), "{run:?}");
        assert_eq!(exit_code(&run), 1);
        let text = render("a", &run);
        assert!(
            text.starts_with("edge-backup: transfer failed, nothing saved"),
            "{text}"
        );
        assert!(text.contains("no caddy"), "{text}");
    }
}

#[tokio::test]
async fn a_download_that_cannot_run_is_a_failed_backup_naming_the_reason() {
    let (s, _) = service("h1", Err("Cannot run ssh"));
    let run = s.backup("a", Path::new("/b"), "t").await.unwrap();
    assert!(
        matches!(&run, BackupRun::Failed(m) if m.contains("Cannot run ssh")),
        "{run:?}"
    );
    assert_eq!(exit_code(&run), 1);
}

#[tokio::test]
async fn an_unknown_server_is_not_found() {
    let (s, log) = service("h1", Ok(Some(0)));
    let e = s.backup("zzz", Path::new("/b"), "t").await.unwrap_err();
    assert!(matches!(e, DomainError::NotFound { .. }), "{e:?}");
    assert!(log.lock().unwrap().is_empty());
}
