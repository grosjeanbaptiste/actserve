use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use actserve::{
    adapters::{
        inbound::http::create_router_full,
        outbound::{
            dns::MockDnsProvider,
            git::git_cli::GitCliProvider,
            memory::{
                job_store::InMemoryJobStore, repo_store::InMemoryRepoStore,
                server_store::InMemoryServerStore,
            },
            registrar::MockRegistrarProvider,
            system::{
                file_inventory::FileServerInventory, ssh_edge_audit::SshEdgeAudit,
                ssh_executor::SshServerExecutor, ssh_hostname_probe::SshHostnameProbe,
            },
            tls::MockTlsManager,
        },
    },
    application::{
        dns_service::DnsService,
        fleet_audit_service::{AuditOutcome, FleetAuditService},
        fleet_status_service::{FleetStatusService, HostReport, all_ok, render},
        orchestration_service::OrchestrationService,
        registrar_service::RegistrarService,
        server_service::ServerService,
    },
};

/// Where the inventory lives (`$ACTSERVE_STATE_ROOT`, default `~/machine`) and
/// which ssh binary to run (`$ACTSERVE_SSH`, default `ssh`).
fn fleet_env() -> Result<(std::path::PathBuf, String), Box<dyn std::error::Error>> {
    let root = std::env::var("ACTSERVE_STATE_ROOT")
        .map(std::path::PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| std::path::Path::new(&h).join("machine")))?;
    let ssh = std::env::var("ACTSERVE_SSH").unwrap_or_else(|_| "ssh".to_string());
    Ok((root, ssh))
}

/// `actserve status [NAME]`: ask each server of the actparc inventory
/// (`$ACTSERVE_STATE_ROOT/servers.txt`, default `~/machine`) for its hostname
/// over ssh (`$ACTSERVE_SSH`, default `ssh`). Exit 0 only when all answer as expected.
async fn run_status(
    name: Option<&str>,
) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let (root, ssh) = fleet_env()?;
    let service = FleetStatusService::new(
        Arc::new(FileServerInventory::new(root.join("servers.txt"))),
        Arc::new(SshHostnameProbe::new(ssh)),
    );
    let reports = service.status(name).await?;
    print!("{}", render(&reports));
    Ok(if all_ok(&reports) {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    })
}

/// `actserve audit NAME`: hostname check, then `edge audit` on the server over
/// ssh with sudo. Exit 0 conforming, 1 deviation / unreachable / other host,
/// 3 unreadable (as `dotfiles servers audit`).
async fn run_audit(name: &str) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let (root, ssh) = fleet_env()?;
    let service = FleetAuditService::new(
        Arc::new(FileServerInventory::new(root.join("servers.txt"))),
        Arc::new(SshHostnameProbe::new(ssh.clone())),
        Arc::new(SshEdgeAudit::new(ssh)),
    );
    let report = service.audit(name).await?;
    match &report.outcome {
        AuditOutcome::NotChecked(status) => print!(
            "{}",
            render(&[HostReport {
                name: report.name.clone(),
                status: status.clone(),
            }])
        ),
        AuditOutcome::Audited(v) => {
            use actserve::domain::fleet::AuditVerdict::*;
            match v {
                Conforming(out) | Deviation(out) => print!("{out}"),
                Unreadable(why) => eprintln!("{}: audit unreadable ({why})", report.name),
            }
        }
    }
    Ok(std::process::ExitCode::from(report.exit_code()))
}

#[tokio::main]
async fn main() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("status") {
        return run_status(args.get(1).map(String::as_str)).await;
    }
    if args.first().map(String::as_str) == Some("audit") {
        let Some(name) = args.get(1) else {
            eprintln!("usage: actserve audit NAME");
            return Ok(std::process::ExitCode::from(2));
        };
        return run_audit(name).await;
    }
    println!(
        "Starting Actserve Orchestrator v{}",
        env!("CARGO_PKG_VERSION")
    );

    let server_store = Arc::new(InMemoryServerStore::new());
    let repo_store = Arc::new(InMemoryRepoStore::new());
    let job_store = Arc::new(InMemoryJobStore::new());
    let executor = Arc::new(SshServerExecutor::new());
    let git_provider = Arc::new(GitCliProvider::default());
    let tofu_engine =
        Arc::new(actserve::adapters::outbound::opentofu::CliOpenTofuAdapter::default());
    let ansible_engine =
        Arc::new(actserve::adapters::outbound::ansible::CliAnsibleAdapter::default());
    let crossplane_engine =
        Arc::new(actserve::adapters::outbound::crossplane::CliCrossplaneAdapter::default());
    let wireguard_engine =
        Arc::new(actserve::adapters::outbound::wireguard::CliWireGuardAdapter::default());
    let dns_provider = Arc::new(MockDnsProvider::default());
    let registrar_provider = Arc::new(MockRegistrarProvider::default());
    let tls_manager = Arc::new(MockTlsManager::default());

    let dns_service = Arc::new(DnsService::new(dns_provider.clone()));
    let registrar_service = Arc::new(RegistrarService::new(registrar_provider));

    let server_service = Arc::new(
        ServerService::new_with_engines(
            server_store.clone(),
            executor.clone(),
            ansible_engine.clone(),
            wireguard_engine,
        )
        .with_dns_provider(dns_provider)
        .with_tls_manager(tls_manager),
    );
    let orchestrator_service = Arc::new(
        OrchestrationService::new(server_store, repo_store, job_store, executor, git_provider)
            .with_tofu_engine(tofu_engine)
            .with_ansible_engine(ansible_engine)
            .with_crossplane_engine(crossplane_engine),
    );

    let router = create_router_full(
        server_service,
        orchestrator_service,
        dns_service,
        registrar_service,
    )
    .layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    println!("Actserve API for Bioxus UI listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(std::process::ExitCode::SUCCESS)
}
