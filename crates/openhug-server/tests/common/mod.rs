use std::{
    net::SocketAddr,
    sync::{Arc, Mutex, OnceLock},
};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::connect_info::MockConnectInfo,
    http::{Request, StatusCode, header},
};
use openhug_server::{
    AppState, build_api_router,
    config::{Config, StorageConfig},
    storage::BlobStore,
};
use serde_json::Value;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tempfile::TempDir;
use tower::ServiceExt;

static TEST_DB_URL: OnceLock<String> = OnceLock::new();

pub fn test_database_url() -> &'static str {
    TEST_DB_URL.get_or_init(|| {
        std::env::var("OPENHUG_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("OPENHUG_DATABASE_URL"))
            .unwrap_or_else(|_| {
                format!(
                    "postgres://{}@localhost/openhug_test",
                    std::env::var("USER").unwrap_or_else(|_| "postgres".into())
                )
            })
    })
}

pub async fn connect_pool() -> PgPool {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(test_database_url())
        .await
        .unwrap_or_else(|error| {
            panic!(
                "failed to connect to test database at {}: {error}\n\
                 start PostgreSQL and create the database, or set OPENHUG_TEST_DATABASE_URL",
                test_database_url()
            )
        });
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

pub async fn reset_database(pool: &PgPool) {
    sqlx::query(
        "TRUNCATE TABLE commit_files, commits, blob_uploads, blobs, repositories, \
         api_tokens, sessions, users, instance_settings RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("reset database");
}

static DB_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn db_test_lock() -> &'static Mutex<()> {
    DB_TEST_LOCK.get_or_init(|| Mutex::new(()))
}

pub struct TestApp {
    _db_guard: std::sync::MutexGuard<'static, ()>,
    pub pool: PgPool,
    pub _storage_dir: TempDir,
    pub state: AppState,
    pub router: Router,
    pub admin_token: String,
    pub admin_username: String,
}

impl TestApp {
    #[allow(clippy::await_holding_lock)]
    pub async fn new() -> Self {
        let db_guard = db_test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let pool = connect_pool().await;
        reset_database(&pool).await;
        let storage_dir = TempDir::new().expect("temp storage dir");
        let config = Arc::new(Config {
            database_url: test_database_url().into(),
            bind: "127.0.0.1:0".parse().unwrap(),
            public_url: "http://localhost:3000".into(),
            setup_token: None,
            storage: StorageConfig::Local {
                path: storage_dir.path().to_path_buf(),
            },
        });
        let storage =
            BlobStore::from_config(&config.storage).expect("initialize test blob storage");
        let state = AppState {
            pool: pool.clone(),
            storage,
            config,
        };
        let router = build_api_router(state.clone())
            .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))));
        let mut app = Self {
            _db_guard: db_guard,
            pool,
            _storage_dir: storage_dir,
            state,
            router,
            admin_token: String::new(),
            admin_username: String::new(),
        };
        app.initialize().await;
        app
    }

    async fn initialize(&mut self) {
        let (_, headers) = json_request(
            &self.router,
            Request::builder()
                .method("POST")
                .uri("/api/v1/setup")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "instance_name": "Test Hub",
                        "username": "admin",
                        "email": "admin@example.com",
                        "password": "test-password-12",
                        "signup_policy": "immediate",
                        "default_visibility": "public",
                        "retention_days": 30
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await;
        let cookie = session_cookie(&headers);
        let token_response = json_request(
            &self.router,
            Request::builder()
                .method("POST")
                .uri("/api/v1/tokens")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie)
                .body(Body::from(
                    serde_json::json!({
                        "name": "test-admin",
                        "scopes": ["read", "write", "admin"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await;
        self.admin_token = token_response.0["token"]
            .as_str()
            .expect("admin token")
            .to_string();
        self.admin_username = "admin".to_string();
    }

    pub fn bearer(&self, token: &str) -> String {
        format!("Bearer {token}")
    }

    pub fn admin_auth(&self) -> String {
        self.bearer(&self.admin_token)
    }
}

pub fn session_cookie(headers: &axum::http::HeaderMap) -> String {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap())
        .find(|value| value.starts_with("openhug_session="))
        .expect("session cookie")
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

pub async fn request(
    app: &Router,
    request: Request<Body>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let response = app.clone().oneshot(request).await.expect("router response");
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body")
        .to_vec();
    (status, headers, body)
}

pub async fn json_request(
    app: &Router,
    http_request: Request<Body>,
) -> (Value, axum::http::HeaderMap) {
    let (status, headers, body) = request(app, http_request).await;
    assert!(
        status.is_success(),
        "expected success but got {status}: {}",
        String::from_utf8_lossy(&body)
    );
    let value: Value = serde_json::from_slice(&body).expect("json body");
    (value, headers)
}

pub async fn json_request_status(app: &Router, http_request: Request<Body>) -> (StatusCode, Value) {
    let (status, _, body) = request(app, http_request).await;
    let value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body)
            .unwrap_or(Value::String(String::from_utf8_lossy(&body).into()))
    };
    (status, value)
}

pub async fn signup_user(app: &TestApp, username: &str, password: &str) -> String {
    let (status, _) = json_request_status(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/v1/auth/signup")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "username": username,
                    "email": format!("{username}@example.com"),
                    "password": password
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    create_user_token(app, username, password).await
}

pub async fn create_user_token(app: &TestApp, username: &str, password: &str) -> String {
    let (_, headers) = json_request(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "identity": username,
                    "password": password
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
            .method("POST")
            .uri("/api/v1/tokens")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, cookie)
            .body(Body::from(
                serde_json::json!({
                    "name": "cli",
                    "scopes": ["read", "write"]
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    value["token"].as_str().expect("user token").to_string()
}

pub async fn create_repository(
    app: &TestApp,
    token: &str,
    kind: &str,
    name: &str,
    visibility: Option<&str>,
) -> Value {
    let mut payload = serde_json::json!({
        "kind": kind,
        "name": name,
        "description": "test repo"
    });
    if let Some(visibility) = visibility {
        payload["visibility"] = Value::String(visibility.into());
    }
    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/v1/repositories")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, app.bearer(token))
            .body(Body::from(payload.to_string()))
            .unwrap(),
    )
    .await;
    value
}

pub async fn upload_blob(app: &TestApp, token: &str, bytes: &[u8]) -> Value {
    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("POST")
            .uri("/api/v1/blobs")
            .header(header::AUTHORIZATION, app.bearer(token))
            .body(Body::from(bytes.to_vec()))
            .unwrap(),
    )
    .await;
    value
}

pub async fn commit_files(
    app: &TestApp,
    token: &str,
    kind: &str,
    owner: &str,
    name: &str,
    files: Value,
    message: &str,
) -> Value {
    let (value, _) = json_request(
        &app.router,
        Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/repositories/{kind}/{owner}/{name}/commits"
            ))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, app.bearer(token))
            .body(Body::from(
                serde_json::json!({
                    "message": message,
                    "files": files,
                    "deletions": []
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    value
}
