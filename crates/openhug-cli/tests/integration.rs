use std::net::SocketAddr;

use axum::{Json, Router, routing::get};
use openhug_cli::{ConfigStore, ServerCommand, handle_server_command};
use serde_json::json;
use tempfile::TempDir;

async fn mock_me() -> Json<serde_json::Value> {
    Json(json!({"username": "alice"}))
}

async fn mock_unauthorized() -> (axum::http::StatusCode, Json<serde_json::Value>) {
    (
        axum::http::StatusCode::UNAUTHORIZED,
        Json(json!({"error": "authentication required"})),
    )
}

async fn start_mock_server(router: Router) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock server");
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("serve mock server");
    });
    (addr, handle)
}

fn temp_store(root: &TempDir) -> ConfigStore {
    ConfigStore {
        config_path: root.path().join("openhug/config.json"),
        legacy_token_path: root.path().join("openhug/token"),
    }
}

#[tokio::test]
async fn server_add_verifies_token_against_mock_server() {
    let (addr, _handle) =
        start_mock_server(Router::new().route("/api/v1/auth/me", get(mock_me))).await;
    let root = TempDir::new().unwrap();
    let store = temp_store(&root);
    handle_server_command(
        &store,
        ServerCommand::Add {
            name: "home".to_string(),
            url: format!("http://{addr}"),
            token: "oh_valid".to_string(),
            default: true,
        },
        false,
    )
    .await
    .expect("add server");
    let config = store.load().unwrap();
    assert_eq!(config.default_server.as_deref(), Some("home"));
    assert_eq!(config.servers["home"].username.as_deref(), Some("alice"));
}

#[tokio::test]
async fn server_add_rejects_invalid_token_without_persisting() {
    let (addr, _handle) =
        start_mock_server(Router::new().route("/api/v1/auth/me", get(mock_unauthorized))).await;
    let root = TempDir::new().unwrap();
    let store = temp_store(&root);
    let error = handle_server_command(
        &store,
        ServerCommand::Add {
            name: "home".to_string(),
            url: format!("http://{addr}"),
            token: "oh_invalid".to_string(),
            default: true,
        },
        false,
    )
    .await
    .expect_err("invalid token should fail");
    assert!(error.to_string().contains("token verification failed"));
    assert!(!store.config_path.exists());
}

#[tokio::test]
async fn server_login_and_logout_update_saved_credentials() {
    let (addr, _handle) =
        start_mock_server(Router::new().route("/api/v1/auth/me", get(mock_me))).await;
    let root = TempDir::new().unwrap();
    let store = temp_store(&root);
    handle_server_command(
        &store,
        ServerCommand::Add {
            name: "home".to_string(),
            url: format!("http://{addr}"),
            token: "oh_first".to_string(),
            default: true,
        },
        false,
    )
    .await
    .unwrap();

    handle_server_command(
        &store,
        ServerCommand::Login {
            name: "home".to_string(),
            token: "oh_second".to_string(),
        },
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        store.load().unwrap().servers["home"].token.as_deref(),
        Some("oh_second")
    );

    handle_server_command(
        &store,
        ServerCommand::Logout {
            name: "home".to_string(),
        },
        false,
    )
    .await
    .unwrap();
    assert!(store.load().unwrap().servers["home"].token.is_none());
}
