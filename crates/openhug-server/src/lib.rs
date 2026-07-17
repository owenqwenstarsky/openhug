pub mod api;
pub mod auth;
pub mod config;
pub mod error;
pub mod storage;

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use config::Config;
use rust_embed::RustEmbed;
use sqlx::PgPool;
use storage::BlobStore;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub storage: BlobStore,
    pub config: Arc<Config>,
}

#[derive(RustEmbed)]
#[folder = "../../web/out"]
struct WebAssets;

pub fn build_api_router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(api::health))
        .route("/api/v1/setup/status", get(api::setup_status))
        .route("/api/v1/setup", post(api::setup))
        .route("/api/v1/auth/login", post(api::login))
        .route("/api/v1/auth/logout", post(api::logout))
        .route("/api/v1/auth/signup", post(api::signup))
        .route("/api/v1/auth/me", get(api::me).put(api::update_me))
        .route(
            "/api/v1/repositories",
            get(api::list_repositories).post(api::create_repository),
        )
        .route(
            "/api/v1/repositories/{kind}/{owner}/{name}",
            get(api::get_repository).delete(api::delete_repository),
        )
        .route(
            "/api/v1/repositories/{kind}/{owner}/{name}/commits",
            get(api::list_commits).post(api::create_commit),
        )
        .route(
            "/api/v1/repositories/{kind}/{owner}/{name}/resolve/{revision}/{*path}",
            get(api::download_file),
        )
        .route(
            "/api/v1/blobs",
            post(api::upload_blob).layer(DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        .route(
            "/api/v1/tokens",
            get(api::list_tokens).post(api::create_token),
        )
        .route("/api/v1/tokens/{id}", delete(api::delete_token))
        .route(
            "/api/v1/admin/settings",
            get(api::get_settings).put(api::update_settings),
        )
        .route("/api/v1/admin/users", get(api::list_users))
        .route(
            "/api/v1/admin/users/{id}",
            axum::routing::patch(api::update_user),
        )
        .route("/api/models/{owner}/{name}", get(api::hf_model_info))
        .route("/api/datasets/{owner}/{name}", get(api::hf_dataset_info))
        .route(
            "/{owner}/{name}/resolve/{revision}/{*path}",
            get(api::hf_model_download),
        )
        .route(
            "/datasets/{owner}/{name}/resolve/{revision}/{*path}",
            get(api::hf_dataset_download),
        )
        .route(
            "/api/{kind}/{owner}/{name}/preupload/{revision}",
            post(api::hf_preupload),
        )
        .route(
            "/api/{kind}/{owner}/{name}/commit/{revision}",
            post(api::hf_commit).layer(DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub fn build_router(state: AppState) -> Router {
    build_api_router(state.clone()).fallback(get(static_handler).with_state(state))
}

pub async fn collect_garbage(state: &AppState) -> anyhow::Result<()> {
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(684621338)")
        .execute(&mut *tx)
        .await?;
    let _removed = sqlx::query("DELETE FROM repositories WHERE deleted_at IS NOT NULL AND deleted_at < now() - ((SELECT retention_days FROM instance_settings) * interval '1 day')")
        .execute(&mut *tx).await?.rows_affected();
    sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM api_tokens WHERE expires_at IS NOT NULL AND expires_at <= now()")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM blob_uploads WHERE expires_at <= now()")
        .execute(&mut *tx)
        .await?;
    let digests = sqlx::query_scalar::<_, String>("SELECT b.sha256 FROM blobs b WHERE b.created_at < now() - interval '24 hours' AND NOT EXISTS (SELECT 1 FROM commit_files f WHERE f.blob_sha256=b.sha256) AND NOT EXISTS (SELECT 1 FROM blob_uploads bu WHERE bu.sha256=b.sha256 AND bu.expires_at > now()) FOR UPDATE")
        .fetch_all(&mut *tx).await?;
    for digest in &digests {
        state.storage.delete(digest).await?;
        sqlx::query("DELETE FROM blobs WHERE sha256=$1 AND NOT EXISTS (SELECT 1 FROM commit_files WHERE blob_sha256=$1)")
            .bind(digest).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn static_handler(State(_): State<AppState>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path == "api" || path.starts_with("api/") {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"error":"API endpoint not found"})),
        )
            .into_response();
    }
    let candidates = [
        path.to_string(),
        format!("{path}.html"),
        format!("{path}/index.html"),
        "index.html".into(),
    ];
    for candidate in candidates {
        if let Some(asset) = WebAssets::get(&candidate) {
            let mime = mime_guess::from_path(&candidate).first_or_octet_stream();
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(asset.data))
                .unwrap();
        }
    }
    StatusCode::NOT_FOUND.into_response()
}
