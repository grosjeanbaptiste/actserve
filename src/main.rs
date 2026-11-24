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
            system::ssh_executor::SshServerExecutor,
            tls::MockTlsManager,
        },
    },
    application::{
        dns_service::DnsService, orchestration_service::OrchestrationService,
        registrar_service::RegistrarService, server_service::ServerService,
    },
    fleet_cli,
};

#[tokio::main]
async fn main() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("status") => return fleet_cli::run_status(args.get(1).map(String::as_str)).await,
        Some("audit") => return fleet_cli::run_audit(args.get(1).map(String::as_str)).await,
        Some(verb @ ("bootstrap" | "apply")) => {
            return fleet_cli::run_bootstrap(verb, &args[1..]).await;
        }
        Some("edge") => return fleet_cli::run_edge(&args[1..]).await,
        Some("stacks") => return fleet_cli::run_stacks(&args[1..]).await,
        Some("runtime") => return fleet_cli::run_runtime(&args[1..]).await,
        Some("harden") => return fleet_cli::run_harden(&args[1..]).await,
        Some("ps") => return actserve::observe_cli::run_ps(&args[1..]).await,
        Some("logs") => return actserve::observe_cli::run_logs(&args[1..]).await,
        Some("deploy") => return actserve::deploy_cli::run_deploy(&args[1..]).await,
        Some("rollback") => return actserve::deploy_cli::run_rollback(&args[1..]).await,
        Some("deploy-caddy") => return actserve::caddy_cli::run_deploy_caddy(&args[1..]).await,
        Some("cdn-deploy") => return actserve::cdn_cli::run_cdn_deploy(&args[1..]).await,
        Some("edge-deploy") => return actserve::edge_cli::run_edge_deploy(&args[1..]).await,
        Some("edge-status") => return actserve::observe_cli::run_edge_status(&args[1..]).await,
        _ => {}
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
