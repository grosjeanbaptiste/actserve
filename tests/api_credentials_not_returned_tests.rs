// A server is registered with SSH credentials (password or private key). The
// API must never hand them back: `Server` is serialized as the response of
// register, get and list. Red before: the secrets appear in every body, so
// each `!body.contains(secret)` assertion fails.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
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

const PASSWORD: &str = "hunter2-the-root-password";
const KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----ZZZ-the-key-body";
const PASSPHRASE: &str = "the-key-passphrase-xyz";

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

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, String) {
    let mut request = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(value) => {
            request = request.header("content-type", "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

fn assert_no_secret(body: &str, where_: &str) {
    for secret in [PASSWORD, KEY, PASSPHRASE] {
        assert!(!body.contains(secret), "{where_} leaks `{secret}`");
    }
}

#[tokio::test]
async fn no_response_returns_the_ssh_credentials() {
    let app = app();
    let mut ids = Vec::new();
    for credentials in [
        json!({ "Password": { "username": "root", "password": PASSWORD } }),
        json!({ "PrivateKey": {
            "username": "root", "key_content": KEY, "passphrase": PASSPHRASE } }),
    ] {
        let (status, body) = call(
            &app,
            "POST",
            "/api/v1/servers",
            Some(json!({
                "name": format!("srv-{}", ids.len()),
                "ip": "192.168.1.50",
                "port": 22,
                "credentials": credentials,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_no_secret(&body, "POST /servers");
        let created: Value = serde_json::from_str(&body).unwrap();
        ids.push(created["id"].as_str().unwrap().to_string());
    }

    let (_, list) = call(&app, "GET", "/api/v1/servers", None).await;
    assert_no_secret(&list, "GET /servers");
    for id in ids {
        let (status, one) = call(&app, "GET", &format!("/api/v1/servers/{id}"), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_no_secret(&one, "GET /servers/{id}");
    }
}
