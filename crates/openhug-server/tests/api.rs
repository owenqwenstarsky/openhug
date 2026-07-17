mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use base64::Engine;
use common::{
    TestApp, commit_files, create_repository, json_request, json_request_status, request,
    session_cookie, signup_user, upload_blob,
};
use openhug_server::collect_garbage;
use serde_json::json;

#[tokio::test]
async fn health_reports_ok() {
    let app = TestApp::new().await;
    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .uri("/api/v1/health")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(value["status"], "ok");
    assert_eq!(value["database"], "connected");
    assert_eq!(value["storage"], "connected");
}

#[tokio::test]
async fn setup_status_reflects_initialized_instance() {
    let app = TestApp::new().await;
    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .uri("/api/v1/setup/status")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(value["initialized"], true);
    assert_eq!(value["instance_name"], "Test Hub");
}

#[tokio::test]
async fn setup_rejects_second_initialization() {
    let app = TestApp::new().await;
    let (status, body) = json_request_status(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/v1/setup")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "instance_name": "Again",
                    "username": "other",
                    "email": "other@example.com",
                    "password": "another-password",
                    "signup_policy": "immediate"
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("already been initialized")
    );
}

#[tokio::test]
async fn auth_login_logout_and_me() {
    let app = TestApp::new().await;
    let (status, _) = json_request_status(
        &app.router,
        Request::builder()
            .method("GET")
            .uri("/api/v1/auth/me")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (_, headers) = json_request(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "identity": "admin",
                    "password": "test-password-12"
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    let cookie = session_cookie(&headers);

    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("GET")
            .uri("/api/v1/auth/me")
            .header(header::COOKIE, cookie.clone())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(value["username"], "admin");

    let (status, _, _) = request(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/v1/auth/logout")
            .header(header::COOKIE, cookie.clone())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = json_request_status(
        &app.router,
        Request::builder()
            .method("GET")
            .uri("/api/v1/auth/me")
            .header(header::COOKIE, cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bearer_token_auth_and_profile_update() {
    let app = TestApp::new().await;
    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("GET")
            .uri("/api/v1/auth/me")
            .header(header::AUTHORIZATION, app.admin_auth())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(value["username"], "admin");

    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("PUT")
            .uri("/api/v1/auth/me")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, app.admin_auth())
            .body(Body::from(json!({"theme": "dark"}).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(value["theme"], "dark");
}

#[tokio::test]
async fn signup_creates_active_user() {
    let app = TestApp::new().await;
    let (status, body) = json_request_status(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/v1/auth/signup")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "username": "alice",
                    "email": "alice@example.com",
                    "password": "alice-password"
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["status"], "active");
}

#[tokio::test]
async fn repository_lifecycle_and_downloads() {
    let app = TestApp::new().await;
    let alice_token = signup_user(&app, "alice", "alice-password").await;
    create_repository(&app, &alice_token, "model", "demo-model", None).await;

    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("GET")
            .uri("/api/v1/repositories?kind=model")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(value.as_array().unwrap().len(), 1);

    let blob = upload_blob(&app, &alice_token, b"hello weights").await;
    commit_files(
        &app,
        &alice_token,
        "model",
        "alice",
        "demo-model",
        json!([{
            "path": "weights.bin",
            "sha256": blob["sha256"],
            "size": blob["size"]
        }]),
        "initial commit",
    )
    .await;

    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("GET")
            .uri("/api/v1/repositories/model/alice/demo-model")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(value["files"].as_array().unwrap().len(), 1);

    let (status, _, body) = request(
        &app.router,
        Request::builder()
            .method("GET")
            .uri("/api/v1/repositories/model/alice/demo-model/resolve/main/weights.bin")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"hello weights");

    let (status, _, _) = request(
        &app.router,
        Request::builder()
            .method("DELETE")
            .uri("/api/v1/repositories/model/alice/demo-model")
            .header(header::AUTHORIZATION, app.bearer(&alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn private_repository_requires_owner() {
    let app = TestApp::new().await;
    let alice_token = signup_user(&app, "alice", "alice-password").await;
    create_repository(
        &app,
        &alice_token,
        "dataset",
        "secret-data",
        Some("private"),
    )
    .await;

    let (status, _) = json_request_status(
        &app.router,
        Request::builder()
            .method("GET")
            .uri("/api/v1/repositories/dataset/alice/secret-data")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("GET")
            .uri("/api/v1/repositories/dataset/alice/secret-data")
            .header(header::AUTHORIZATION, app.bearer(&alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(value["visibility"], "private");
}

#[tokio::test]
async fn token_management() {
    let app = TestApp::new().await;
    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("GET")
            .uri("/api/v1/tokens")
            .header(header::AUTHORIZATION, app.admin_auth())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert!(!value.as_array().unwrap().is_empty());

    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/v1/tokens")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, app.admin_auth())
            .body(Body::from(
                json!({"name": "read-only", "scopes": ["read"]}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    let token_id = value["id"].as_str().unwrap();
    let read_token = value["token"].as_str().unwrap();

    let (status, _) = json_request_status(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/v1/repositories")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, app.bearer(read_token))
            .body(Body::from(
                json!({"kind": "model", "name": "blocked"}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _, _) = request(
        &app.router,
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/tokens/{token_id}"))
            .header(header::AUTHORIZATION, app.admin_auth())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn admin_settings_and_users() {
    let app = TestApp::new().await;
    let alice_token = signup_user(&app, "alice", "alice-password").await;

    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("GET")
            .uri("/api/v1/admin/settings")
            .header(header::AUTHORIZATION, app.admin_auth())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(value["instance_name"], "Test Hub");

    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("PUT")
            .uri("/api/v1/admin/settings")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, app.admin_auth())
            .body(Body::from(
                json!({
                    "instance_name": "Updated Hub",
                    "signup_policy": "approval",
                    "default_visibility": "private",
                    "retention_days": 14
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(value["instance_name"], "Updated Hub");

    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("GET")
            .uri("/api/v1/admin/users")
            .header(header::AUTHORIZATION, app.admin_auth())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert!(value.as_array().unwrap().len() >= 2);

    let alice_id = value
        .as_array()
        .unwrap()
        .iter()
        .find(|user| user["username"] == "alice")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, _, _) = request(
        &app.router,
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/admin/users/{alice_id}"))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, app.admin_auth())
            .body(Body::from(json!({"status": "suspended"}).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = json_request_status(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"identity": "alice", "password": "alice-password"}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let _ = alice_token;
}

#[tokio::test]
async fn huggingface_compatible_endpoints() {
    let app = TestApp::new().await;
    let alice_token = signup_user(&app, "alice", "alice-password").await;
    create_repository(&app, &alice_token, "model", "hf-model", None).await;
    let blob = upload_blob(&app, &alice_token, b"hf payload").await;
    commit_files(
        &app,
        &alice_token,
        "model",
        "alice",
        "hf-model",
        json!([{
            "path": "model.bin",
            "sha256": blob["sha256"],
            "size": blob["size"]
        }]),
        "hf commit",
    )
    .await;

    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("GET")
            .uri("/api/models/alice/hf-model")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(value["id"], "alice/hf-model");
    assert_eq!(value["siblings"].as_array().unwrap().len(), 1);

    let (status, _, body) = request(
        &app.router,
        Request::builder()
            .method("GET")
            .uri("/alice/hf-model/resolve/main/model.bin")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"hf payload");

    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/models/alice/hf-model/preupload/main")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, app.bearer(&alice_token))
            .body(Body::from(
                json!({"files": [{"path": "extra.txt", "size": 12}]}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(value["files"][0]["uploadMode"], "regular");

    let ndjson = format!(
        "{}\n",
        json!({
            "key": "file",
            "value": {
                "path": "extra.txt",
                "content": base64::engine::general_purpose::STANDARD.encode(b"inline content")
            }
        })
    );
    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/models/alice/hf-model/commit/main")
            .header(header::AUTHORIZATION, app.bearer(&alice_token))
            .body(Body::from(ndjson))
            .unwrap(),
    )
    .await;
    assert!(value["commit"].is_string());
}

#[tokio::test]
async fn garbage_collection_removes_expired_sessions_and_orphan_blobs() {
    let app = TestApp::new().await;
    let alice_token = signup_user(&app, "alice", "alice-password").await;
    let blob = upload_blob(&app, &alice_token, b"orphan blob").await;

    sqlx::query("UPDATE blob_uploads SET expires_at = now() - interval '1 hour'")
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET expires_at = now() - interval '1 hour'")
        .execute(&app.pool)
        .await
        .unwrap();

    collect_garbage(&app.state).await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM blob_uploads WHERE sha256 = $1")
        .bind(blob["sha256"].as_str().unwrap())
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn validation_errors_return_bad_request() {
    let app = TestApp::new().await;
    let (status, body) = json_request_status(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/v1/auth/signup")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "username": "ab",
                    "email": "bad@example.com",
                    "password": "short"
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("username"));

    let (status, body) = json_request_status(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/v1/repositories")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, app.admin_auth())
            .body(Body::from(
                json!({"kind": "model", "name": "../bad"}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("invalid repository name")
    );
}
