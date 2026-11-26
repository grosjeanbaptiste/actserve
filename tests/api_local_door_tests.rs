// The HTTP API commands real servers over SSH (`POST /api/v1/servers/{id}/execute`,
// `/reboot`, ...) and has no login. Its only door is the network, so the door
// must refuse what a web page can send: any request carrying `Origin`, and any
// request whose `Host` is not loopback (DNS rebinding). A CLI or the Bioxus
// desktop client sends neither. Red before: `create_router` answers all of
// these 200, so each refusal assertion below fails with 200 != 403.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use std::sync::Arc;
use tower::ServiceExt;

use actserve::{
    adapters::{
        inbound::http::create_router,
        outbound::{
            memory::{
                job_store::InMemoryJobStore, repo_store::InMemoryRepoStore,
                server_store::InMemoryServerStore,
            },
            mock::{MockGitProvider, MockServerExecutor},
        },
    },
    application::{orchestration_service::OrchestrationService, server_service::ServerService},
    ports::inbound::{repo_orchestrator::RepoOrchestratorPort, server_admin::ServerAdminPort},
};

fn app() -> axum::Router {
    let server_store = Arc::new(InMemoryServerStore::new());
    let repo_store = Arc::new(InMemoryRepoStore::new());
    let job_store = Arc::new(InMemoryJobStore::new());
    let executor = Arc::new(MockServerExecutor::default());
    let git_provider = Arc::new(MockGitProvider::default());
    let server_service: Arc<dyn ServerAdminPort> =
        Arc::new(ServerService::new(server_store.clone(), executor.clone()));
    let orchestrator: Arc<dyn RepoOrchestratorPort> = Arc::new(OrchestrationService::new(
        server_store,
        repo_store,
        job_store,
        executor,
        git_provider,
    ));
    create_router(server_service, orchestrator)
}

async fn status(method: Method, uri: &str, headers: &[(&str, &str)]) -> (StatusCode, bool) {
    let mut request = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = app()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let cors = response
        .headers()
        .contains_key("access-control-allow-origin");
    (response.status(), cors)
}

#[tokio::test]
async fn a_web_page_request_carrying_origin_is_refused() {
    let (code, _) = status(
        Method::GET,
        "/api/v1/servers",
        &[("origin", "https://evil.example")],
    )
    .await;
    assert_eq!(code, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_cross_origin_post_to_execute_is_refused_before_any_route() {
    let (code, _) = status(
        Method::POST,
        "/api/v1/servers/550e8400-e29b-41d4-a716-446655440000/execute",
        &[("origin", "https://evil.example")],
    )
    .await;
    assert_eq!(code, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_preflight_is_refused_and_grants_no_cors() {
    let (code, cors) = status(
        Method::OPTIONS,
        "/api/v1/servers",
        &[
            ("origin", "https://evil.example"),
            ("access-control-request-method", "POST"),
        ],
    )
    .await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    assert!(!cors, "a refused preflight must not carry CORS headers");
}

#[tokio::test]
async fn a_foreign_host_is_refused_even_without_origin() {
    // DNS rebinding: the page's own domain resolves to 127.0.0.1, so the
    // browser sends no Origin on a same-site GET but keeps the foreign Host.
    let (code, _) = status(
        Method::GET,
        "/api/v1/servers",
        &[("host", "rebind.evil.example:8080")],
    )
    .await;
    assert_eq!(code, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_host_that_only_starts_like_loopback_is_refused() {
    let (code, _) = status(
        Method::GET,
        "/api/v1/servers",
        &[("host", "127.0.0.1.evil.example")],
    )
    .await;
    assert_eq!(code, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn loopback_hosts_and_no_host_still_pass() {
    for host in [
        "127.0.0.1:8080",
        "localhost:8080",
        "localhost",
        "[::1]:8080",
    ] {
        let (code, _) = status(Method::GET, "/api/v1/health", &[("host", host)]).await;
        assert_eq!(code, StatusCode::OK, "host {host} must pass");
    }
    let (code, _) = status(Method::GET, "/api/v1/health", &[]).await;
    assert_eq!(code, StatusCode::OK, "a client with no Host must pass");
}
