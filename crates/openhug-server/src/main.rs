use std::sync::Arc;

use anyhow::Result;
use openhug_server::{AppState, build_router, collect_garbage, config::Config, storage::BlobStore};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openhug_server=info,tower_http=info".into()),
        )
        .init();
    let config = Arc::new(Config::from_env()?);
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;
    let storage = BlobStore::from_config(&config.storage)?;
    storage.healthcheck().await?;
    let referenced_digests = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT b.sha256 FROM blobs b JOIN commit_files f ON f.blob_sha256=b.sha256",
    )
    .fetch_all(&pool)
    .await?;
    for digest in referenced_digests {
        if !storage.contains(&digest).await? {
            anyhow::bail!(
                "the configured storage backend does not contain blobs referenced by PostgreSQL; restore the previous environment configuration or migrate storage before switching"
            );
        }
    }
    let state = AppState {
        pool,
        storage,
        config: config.clone(),
    };
    tokio::spawn(garbage_collection_loop(state.clone()));
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(address=%config.bind, storage=%config.storage_label(), "OpenHug listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}

async fn garbage_collection_loop(state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
    loop {
        interval.tick().await;
        if let Err(error) = collect_garbage(&state).await {
            tracing::error!(%error, "repository retention cleanup failed");
        }
    }
}
