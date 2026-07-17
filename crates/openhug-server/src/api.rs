use axum::{
    Json,
    body::Bytes,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use password_auth::{generate_hash, verify_password};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, Transaction};
use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};
use uuid::Uuid;

use crate::{
    AppState,
    auth::{CurrentUser, create_session, hash_secret, random_secret},
    error::{AppError, AppResult},
    storage::blob_key,
};

static LOGIN_ATTEMPTS: LazyLock<Mutex<HashMap<String, VecDeque<Instant>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static DUMMY_PASSWORD_HASH: LazyLock<String> =
    LazyLock::new(|| generate_hash("OpenHug dummy password verification value"));
const LOGIN_WINDOW: Duration = Duration::from_secs(60);
const MAX_LOGIN_ATTEMPTS_PER_WINDOW: usize = 10;

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
    database: &'static str,
    storage: &'static str,
    storage_driver: String,
}

pub async fn health(State(state): State<AppState>) -> AppResult<Json<Health>> {
    sqlx::query("SELECT 1").execute(&state.pool).await?;
    state
        .storage
        .healthcheck()
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(Health {
        status: "ok",
        database: "connected",
        storage: "connected",
        storage_driver: state.config.storage_label().into(),
    }))
}

#[derive(Serialize)]
pub struct SetupStatus {
    initialized: bool,
    instance_name: Option<String>,
    signup_policy: Option<String>,
    default_visibility: Option<String>,
    setup_token_required: bool,
}

pub async fn setup_status(State(state): State<AppState>) -> AppResult<Json<SetupStatus>> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT instance_name,signup_policy,default_visibility::text FROM instance_settings WHERE singleton=true",
    )
    .fetch_optional(&state.pool)
    .await?;
    Ok(Json(SetupStatus {
        initialized: row.is_some(),
        instance_name: row.as_ref().map(|r| r.0.clone()),
        signup_policy: row.as_ref().map(|r| r.1.clone()),
        default_visibility: row.map(|r| r.2),
        setup_token_required: state.config.setup_token.is_some(),
    }))
}

#[derive(Deserialize)]
pub struct SetupInput {
    instance_name: String,
    username: String,
    email: String,
    password: String,
    signup_policy: String,
    #[serde(default = "default_visibility")]
    default_visibility: String,
    #[serde(default = "default_retention")]
    retention_days: i32,
    setup_token: Option<String>,
}
fn default_visibility() -> String {
    "public".into()
}
fn default_retention() -> i32 {
    30
}

pub async fn setup(
    State(state): State<AppState>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    Json(input): Json<SetupInput>,
) -> AppResult<impl IntoResponse> {
    authorize_setup(&state, Some(remote_addr), input.setup_token.as_deref())?;
    validate_username(&input.username)?;
    validate_password(&input.password)?;
    if input.instance_name.trim().is_empty() || input.instance_name.trim().len() > 80 {
        return Err(AppError::BadRequest("invalid instance name".into()));
    }
    if !["disabled", "immediate", "approval"].contains(&input.signup_policy.as_str()) {
        return Err(AppError::BadRequest("invalid signup policy".into()));
    }
    if !["public", "private"].contains(&input.default_visibility.as_str())
        || !(1..=3650).contains(&input.retention_days)
    {
        return Err(AppError::BadRequest("invalid setup settings".into()));
    }
    state
        .storage
        .healthcheck()
        .await
        .map_err(AppError::Internal)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(684621337)")
        .execute(&mut *tx)
        .await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM instance_settings)")
        .fetch_one(&mut *tx)
        .await?;
    if exists {
        return Err(AppError::Conflict(
            "this instance has already been initialized".into(),
        ));
    }
    let hash = generate_hash(&input.password);
    let user_id: Uuid = sqlx::query_scalar("INSERT INTO users (username,email,password_hash,role,status) VALUES ($1,$2,$3,'superuser','active') RETURNING id")
        .bind(input.username.to_lowercase()).bind(input.email.to_lowercase()).bind(hash).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO instance_settings (instance_name,signup_policy,default_visibility,retention_days) VALUES ($1,$2,$3::text::repository_visibility,$4)")
        .bind(input.instance_name.trim()).bind(input.signup_policy).bind(input.default_visibility).bind(input.retention_days).execute(&mut *tx).await?;
    tx.commit().await?;
    let session = create_session(&state.pool, user_id).await?;
    Ok(session_response(
        StatusCode::CREATED,
        session,
        serde_json::json!({"initialized": true}),
        state.config.public_url.starts_with("https://"),
    ))
}

#[derive(Deserialize)]
pub struct LoginInput {
    identity: String,
    password: String,
}

#[derive(FromRow)]
struct LoginRow {
    id: Uuid,
    password_hash: String,
    status: String,
}

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    Json(input): Json<LoginInput>,
) -> AppResult<impl IntoResponse> {
    check_login_throttle(Some(remote_addr), input.identity.trim())?;
    let row = sqlx::query_as::<_, LoginRow>("SELECT id,password_hash,status::text AS status FROM users WHERE lower(email)=lower($1) OR username=lower($1)")
        .bind(input.identity.trim()).fetch_optional(&state.pool).await?;
    let password_hash = row
        .as_ref()
        .map(|row| row.password_hash.as_str())
        .unwrap_or(DUMMY_PASSWORD_HASH.as_str());
    verify_password(input.password, password_hash).map_err(|_| AppError::Unauthorized)?;
    let row = row.ok_or(AppError::Unauthorized)?;
    if row.status != "active" {
        return Err(AppError::Forbidden);
    }
    let session = create_session(&state.pool, row.id).await?;
    Ok(session_response(
        StatusCode::OK,
        session,
        serde_json::json!({"authenticated": true}),
        state.config.public_url.starts_with("https://"),
    ))
}

fn session_response(
    status: StatusCode,
    session: String,
    body: serde_json::Value,
    secure: bool,
) -> impl IntoResponse {
    let secure = if secure { "; Secure" } else { "" };
    let cookie = format!(
        "openhug_session={session}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000{secure}"
    );
    (status, [(header::SET_COOKIE, cookie)], Json(body))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    if let Some(session) = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|c| {
            c.split(';')
                .find_map(|p| p.trim().strip_prefix("openhug_session="))
        })
    {
        sqlx::query("DELETE FROM sessions WHERE id_hash=$1")
            .bind(hash_secret(session))
            .execute(&state.pool)
            .await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            "openhug_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax",
        )],
    ))
}

pub async fn me(user: CurrentUser) -> Json<CurrentUser> {
    Json(user)
}

#[derive(Deserialize)]
pub struct UpdateMe {
    theme: Option<String>,
}

pub async fn update_me(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(input): Json<UpdateMe>,
) -> AppResult<Json<CurrentUser>> {
    let theme = input.theme.unwrap_or_else(|| user.theme.clone());
    if !["light", "dark"].contains(&theme.as_str()) {
        return Err(AppError::BadRequest("invalid theme".into()));
    }
    sqlx::query("UPDATE users SET theme=$1 WHERE id=$2")
        .bind(&theme)
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    let mut user = user;
    user.theme = theme;
    Ok(Json(user))
}

#[derive(Deserialize)]
pub struct SignupInput {
    username: String,
    email: String,
    password: String,
}

pub async fn signup(
    State(state): State<AppState>,
    Json(input): Json<SignupInput>,
) -> AppResult<impl IntoResponse> {
    validate_username(&input.username)?;
    validate_password(&input.password)?;
    let policy: Option<String> = sqlx::query_scalar("SELECT signup_policy FROM instance_settings")
        .fetch_optional(&state.pool)
        .await?;
    let policy =
        policy.ok_or_else(|| AppError::Conflict("instance setup is not complete".into()))?;
    if policy == "disabled" {
        return Err(AppError::Forbidden);
    }
    let status = if policy == "approval" {
        "pending"
    } else {
        "active"
    };
    let result = sqlx::query("INSERT INTO users (username,email,password_hash,status) VALUES ($1,$2,$3,$4::text::user_status)")
        .bind(input.username.to_lowercase()).bind(input.email.to_lowercase()).bind(generate_hash(input.password)).bind(status).execute(&state.pool).await;
    match result {
        Ok(_) => Ok((
            StatusCode::CREATED,
            Json(serde_json::json!({"status": status})),
        )
            .into_response()),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(AppError::Conflict(
            "username or email is already in use".into(),
        )),
        Err(e) => Err(e.into()),
    }
}

#[derive(Serialize, FromRow)]
pub struct Repository {
    id: Uuid,
    owner: String,
    kind: String,
    name: String,
    description: String,
    visibility: String,
    head_commit_id: Option<Uuid>,
    download_count: i64,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize, Default)]
pub struct RepositoryQuery {
    kind: Option<String>,
    search: Option<String>,
    owner: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn list_repositories(
    State(state): State<AppState>,
    user: OptionUser,
    Query(q): Query<RepositoryQuery>,
) -> AppResult<Json<Vec<Repository>>> {
    let user_id = user.0.map(|u| u.id);
    let rows = sqlx::query_as::<_, Repository>(r#"
        SELECT r.id,u.username AS owner,r.kind::text AS kind,r.name,r.description,r.visibility::text AS visibility,r.head_commit_id,r.download_count,r.updated_at
        FROM repositories r JOIN users u ON u.id=r.owner_id
        WHERE r.deleted_at IS NULL AND ($1::text IS NULL OR r.kind::text=$1)
          AND ($2::text IS NULL OR r.name ILIKE '%' || $2 || '%' OR r.description ILIKE '%' || $2 || '%')
          AND ($3::text IS NULL OR u.username=$3)
          AND (r.visibility='public' OR r.owner_id=$4)
        ORDER BY r.updated_at DESC LIMIT $5 OFFSET $6"#)
        .bind(q.kind).bind(q.search).bind(q.owner).bind(user_id).bind(q.limit.unwrap_or(30).clamp(1,100)).bind(q.offset.unwrap_or(0).max(0))
        .fetch_all(&state.pool).await?;
    Ok(Json(rows))
}

pub struct OptionUser(pub Option<CurrentUser>);
impl axum::extract::FromRequestParts<AppState> for OptionUser {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = crate::auth::authenticate(
            &state.pool,
            parts
                .headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok()),
            parts
                .headers
                .get(header::COOKIE)
                .and_then(|v| v.to_str().ok()),
        )
        .await
        .ok()
        .filter(|user| user.require_scope("read").is_ok());
        Ok(Self(user))
    }
}

#[derive(Deserialize)]
pub struct CreateRepository {
    kind: String,
    name: String,
    #[serde(default)]
    description: String,
    visibility: Option<String>,
}

pub async fn create_repository(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(input): Json<CreateRepository>,
) -> AppResult<impl IntoResponse> {
    user.require_scope("write")?;
    if !["model", "dataset"].contains(&input.kind.as_str())
        || input
            .visibility
            .as_deref()
            .is_some_and(|visibility| !["public", "private"].contains(&visibility))
    {
        return Err(AppError::BadRequest(
            "invalid repository kind or visibility".into(),
        ));
    }
    validate_repo_name(&input.name)?;
    let mut tx = state.pool.begin().await?;
    let visibility = match input.visibility {
        Some(visibility) => visibility,
        None => {
            sqlx::query_scalar::<_, String>(
                "SELECT default_visibility::text FROM instance_settings WHERE singleton=true",
            )
            .fetch_one(&mut *tx)
            .await?
        }
    };
    let result = sqlx::query_scalar("INSERT INTO repositories (owner_id,kind,name,description,visibility) VALUES ($1,$2::text::repository_kind,$3,$4,$5::text::repository_visibility) RETURNING id")
        .bind(user.id).bind(input.kind).bind(input.name).bind(input.description).bind(visibility).fetch_one(&mut *tx).await;
    let id: Uuid = match result {
        Ok(id) => id,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(AppError::Conflict("repository already exists".into()));
        }
        Err(e) => return Err(e.into()),
    };
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"id": id}))))
}

async fn find_repo(
    state: &AppState,
    kind: &str,
    owner: &str,
    name: &str,
    user_id: Option<Uuid>,
) -> AppResult<Repository> {
    sqlx::query_as::<_, Repository>(r#"SELECT r.id,u.username AS owner,r.kind::text AS kind,r.name,r.description,r.visibility::text AS visibility,r.head_commit_id,r.download_count,r.updated_at FROM repositories r JOIN users u ON u.id=r.owner_id WHERE r.kind::text=$1 AND u.username=$2 AND r.name=$3 AND r.deleted_at IS NULL AND (r.visibility='public' OR r.owner_id=$4)"#)
        .bind(kind).bind(owner).bind(name).bind(user_id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)
}

#[derive(Serialize, FromRow)]
pub struct RepoFile {
    path: String,
    sha256: String,
    size: i64,
}
#[derive(Serialize)]
pub struct RepositoryDetail {
    #[serde(flatten)]
    repository: Repository,
    files: Vec<RepoFile>,
}

pub async fn get_repository(
    State(state): State<AppState>,
    user: OptionUser,
    Path((kind, owner, name)): Path<(String, String, String)>,
) -> AppResult<Json<RepositoryDetail>> {
    let repo = find_repo(&state, &kind, &owner, &name, user.0.map(|u| u.id)).await?;
    let files = if let Some(head) = repo.head_commit_id {
        sqlx::query_as::<_, RepoFile>("SELECT path,blob_sha256 AS sha256,size FROM commit_files WHERE commit_id=$1 ORDER BY path").bind(head).fetch_all(&state.pool).await?
    } else {
        vec![]
    };
    Ok(Json(RepositoryDetail {
        repository: repo,
        files,
    }))
}

#[derive(Serialize)]
pub struct BlobReceipt {
    sha256: String,
    size: i64,
}
pub async fn upload_blob(
    State(state): State<AppState>,
    user: CurrentUser,
    body: Bytes,
) -> AppResult<Json<BlobReceipt>> {
    user.require_scope("write")?;
    if body.len() > 512 * 1024 * 1024usize {
        return Err(AppError::BadRequest(
            "blob exceeds the 512 MiB v1 direct-upload limit".into(),
        ));
    }
    let (sha256, size) = state.storage.put(body).await.map_err(AppError::Internal)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("INSERT INTO blobs (sha256,size,storage_key) VALUES ($1,$2,$3) ON CONFLICT (sha256) DO NOTHING")
        .bind(&sha256).bind(size).bind(blob_key(&sha256)).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO blob_uploads (user_id,sha256,size,expires_at) VALUES ($1,$2,$3,$4) ON CONFLICT (user_id,sha256) DO UPDATE SET size=excluded.size,expires_at=excluded.expires_at,created_at=now()")
        .bind(user.id).bind(&sha256).bind(size).bind(Utc::now() + ChronoDuration::hours(24)).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(BlobReceipt { sha256, size }))
}

#[derive(Deserialize)]
pub struct CommitFile {
    path: String,
    sha256: String,
    size: i64,
}
#[derive(Deserialize)]
pub struct CreateCommit {
    message: String,
    #[serde(default)]
    files: Vec<CommitFile>,
    #[serde(default)]
    deletions: Vec<String>,
}

pub async fn create_commit(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((kind, owner, name)): Path<(String, String, String)>,
    Json(input): Json<CreateCommit>,
) -> AppResult<impl IntoResponse> {
    user.require_scope("write")?;
    if input.message.trim().is_empty() || input.message.trim().len() > 500 {
        return Err(AppError::BadRequest(
            "commit message must be 1–500 characters".into(),
        ));
    }
    let mut tx = state.pool.begin().await?;
    let (repo_id, head, owner_id): (Uuid,Option<Uuid>,Uuid) = sqlx::query_as("SELECT r.id,r.head_commit_id,r.owner_id FROM repositories r JOIN users u ON u.id=r.owner_id WHERE r.kind::text=$1 AND u.username=$2 AND r.name=$3 AND r.deleted_at IS NULL FOR UPDATE")
        .bind(&kind).bind(&owner).bind(&name).fetch_optional(&mut *tx).await?.ok_or(AppError::NotFound)?;
    if owner_id != user.id && !user.is_superuser() {
        return Err(AppError::Forbidden);
    }
    for file in &input.files {
        validate_path(&file.path)?;
    }
    for path in &input.deletions {
        validate_path(path)?;
    }
    let commit_id: Uuid = sqlx::query_scalar("INSERT INTO commits (repository_id,parent_id,author_id,message) VALUES ($1,$2,$3,$4) RETURNING id")
        .bind(repo_id).bind(head).bind(user.id).bind(input.message.trim()).fetch_one(&mut *tx).await?;
    if let Some(head) = head {
        sqlx::query("INSERT INTO commit_files (commit_id,path,blob_sha256,size) SELECT $1,path,blob_sha256,size FROM commit_files WHERE commit_id=$2")
            .bind(commit_id).bind(head).execute(&mut *tx).await?;
    }
    for path in input.deletions {
        sqlx::query("DELETE FROM commit_files WHERE commit_id=$1 AND path=$2")
            .bind(commit_id)
            .bind(path)
            .execute(&mut *tx)
            .await?;
    }
    let mut consumed_uploads = Vec::new();
    for file in input.files {
        let stored_size: Option<i64> = sqlx::query_scalar(
            "SELECT b.size FROM blobs b JOIN blob_uploads bu ON bu.sha256=b.sha256 WHERE b.sha256=$1 AND bu.user_id=$2 AND bu.expires_at > now() FOR SHARE OF b",
        )
        .bind(&file.sha256)
        .bind(user.id)
        .fetch_optional(&mut *tx)
        .await?;
        if stored_size != Some(file.size) {
            return Err(AppError::BadRequest(format!(
                "blob {} has not been uploaded by this user or the upload receipt expired",
                file.sha256
            )));
        }
        consumed_uploads.push(file.sha256.clone());
        sqlx::query("INSERT INTO commit_files (commit_id,path,blob_sha256,size) VALUES ($1,$2,$3,$4) ON CONFLICT (commit_id,path) DO UPDATE SET blob_sha256=excluded.blob_sha256,size=excluded.size")
            .bind(commit_id).bind(file.path).bind(file.sha256).bind(file.size).execute(&mut *tx).await?;
    }
    consumed_uploads.sort();
    consumed_uploads.dedup();
    for sha256 in consumed_uploads {
        sqlx::query("DELETE FROM blob_uploads WHERE user_id=$1 AND sha256=$2")
            .bind(user.id)
            .bind(sha256)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE repositories SET head_commit_id=$1,updated_at=now() WHERE id=$2")
        .bind(commit_id)
        .bind(repo_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"commit":commit_id,"revision":"main"})),
    ))
}

#[derive(Serialize, FromRow)]
pub struct CommitView {
    id: Uuid,
    parent_id: Option<Uuid>,
    author: String,
    message: String,
    created_at: DateTime<Utc>,
}

pub async fn list_commits(
    State(state): State<AppState>,
    user: OptionUser,
    Path((kind, owner, name)): Path<(String, String, String)>,
) -> AppResult<Json<Vec<CommitView>>> {
    let repo = find_repo(&state, &kind, &owner, &name, user.0.map(|u| u.id)).await?;
    Ok(Json(sqlx::query_as("SELECT c.id,c.parent_id,u.username AS author,c.message,c.created_at FROM commits c JOIN users u ON u.id=c.author_id WHERE c.repository_id=$1 ORDER BY c.created_at DESC LIMIT 100")
        .bind(repo.id).fetch_all(&state.pool).await?))
}

pub async fn download_file(
    State(state): State<AppState>,
    user: OptionUser,
    Path((kind, owner, name, revision, path)): Path<(String, String, String, String, String)>,
) -> AppResult<impl IntoResponse> {
    let repo = find_repo(&state, &kind, &owner, &name, user.0.map(|u| u.id)).await?;
    let commit = if revision == "main" {
        repo.head_commit_id.ok_or(AppError::NotFound)?
    } else {
        Uuid::parse_str(&revision).map_err(|_| AppError::NotFound)?
    };
    let sha: String = sqlx::query_scalar(
        "SELECT f.blob_sha256 FROM commit_files f JOIN commits c ON c.id=f.commit_id WHERE f.commit_id=$1 AND f.path=$2 AND c.repository_id=$3",
    )
    .bind(commit)
    .bind(&path)
    .bind(repo.id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;
    let bytes = state
        .storage
        .get(&sha)
        .await
        .map_err(|_| AppError::NotFound)?;
    sqlx::query("UPDATE repositories SET download_count=download_count+1 WHERE id=$1")
        .bind(repo.id)
        .execute(&state.pool)
        .await?;
    let headers = download_headers(&path, commit)?;
    Ok((headers, bytes))
}

#[derive(Deserialize)]
pub struct CreateToken {
    name: String,
    #[serde(default = "default_scopes")]
    scopes: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
}
fn default_scopes() -> Vec<String> {
    vec!["read".into(), "write".into()]
}
pub async fn create_token(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(input): Json<CreateToken>,
) -> AppResult<impl IntoResponse> {
    user.require_scope("write")?;
    if input.name.trim().is_empty() || input.name.trim().len() > 120 {
        return Err(AppError::BadRequest("invalid token name".into()));
    }
    if input.scopes.iter().any(|s| {
        !["read", "write", "admin"].contains(&s.as_str()) || s == "admin" && !user.is_superuser()
    }) {
        return Err(AppError::BadRequest("invalid token scope".into()));
    }
    if input.scopes.iter().any(|scope| scope == "admin") {
        user.require_scope("admin")?;
    }
    let secret = random_secret("oh_");
    let id:Uuid=sqlx::query_scalar("INSERT INTO api_tokens (user_id,name,token_hash,scopes,expires_at) VALUES ($1,$2,$3,$4,$5) RETURNING id")
        .bind(user.id).bind(input.name).bind(hash_secret(&secret)).bind(input.scopes).bind(input.expires_at).fetch_one(&state.pool).await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"id":id,"token":secret})),
    ))
}

#[derive(Serialize, FromRow)]
pub struct TokenView {
    id: Uuid,
    name: String,
    scopes: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
    last_used_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}
pub async fn list_tokens(
    State(state): State<AppState>,
    user: CurrentUser,
) -> AppResult<Json<Vec<TokenView>>> {
    user.require_scope("read")?;
    Ok(Json(sqlx::query_as("SELECT id,name,scopes,expires_at,last_used_at,created_at FROM api_tokens WHERE user_id=$1 ORDER BY created_at DESC").bind(user.id).fetch_all(&state.pool).await?))
}
pub async fn delete_token(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    user.require_scope("write")?;
    sqlx::query("DELETE FROM api_tokens WHERE id=$1 AND user_id=$2")
        .bind(id)
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HfRepoInfo {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_id: Option<String>,
    sha: Option<Uuid>,
    private: bool,
    downloads: i64,
    siblings: Vec<HfSibling>,
}

#[derive(Serialize)]
pub struct HfSibling {
    rfilename: String,
}

async fn hf_info(
    state: &AppState,
    user: OptionUser,
    kind: &str,
    owner: &str,
    name: &str,
) -> AppResult<Json<HfRepoInfo>> {
    let repo = find_repo(state, kind, owner, name, user.0.map(|u| u.id)).await?;
    let files = if let Some(head) = repo.head_commit_id {
        sqlx::query_as::<_, RepoFile>(
            "SELECT path,blob_sha256 AS sha256,size FROM commit_files WHERE commit_id=$1 ORDER BY path",
        )
        .bind(head)
        .fetch_all(&state.pool)
        .await?
    } else {
        vec![]
    };
    let id = format!("{owner}/{name}");
    Ok(Json(HfRepoInfo {
        model_id: (kind == "model").then(|| id.clone()),
        id,
        sha: repo.head_commit_id,
        private: repo.visibility == "private",
        downloads: repo.download_count,
        siblings: files
            .into_iter()
            .map(|file| HfSibling {
                rfilename: file.path,
            })
            .collect(),
    }))
}

pub async fn hf_model_info(
    State(state): State<AppState>,
    user: OptionUser,
    Path((owner, name)): Path<(String, String)>,
) -> AppResult<Json<HfRepoInfo>> {
    hf_info(&state, user, "model", &owner, &name).await
}

pub async fn hf_dataset_info(
    State(state): State<AppState>,
    user: OptionUser,
    Path((owner, name)): Path<(String, String)>,
) -> AppResult<Json<HfRepoInfo>> {
    hf_info(&state, user, "dataset", &owner, &name).await
}

pub async fn hf_model_download(
    state: State<AppState>,
    user: OptionUser,
    Path((owner, name, revision, path)): Path<(String, String, String, String)>,
) -> AppResult<impl IntoResponse> {
    download_file(
        state,
        user,
        Path(("model".into(), owner, name, revision, path)),
    )
    .await
}

pub async fn hf_dataset_download(
    state: State<AppState>,
    user: OptionUser,
    Path((owner, name, revision, path)): Path<(String, String, String, String)>,
) -> AppResult<impl IntoResponse> {
    download_file(
        state,
        user,
        Path(("dataset".into(), owner, name, revision, path)),
    )
    .await
}

#[derive(Deserialize)]
pub struct HfPreuploadInput {
    files: Vec<HfPreuploadFile>,
}
#[derive(Deserialize)]
pub struct HfPreuploadFile {
    path: String,
    size: i64,
}

pub async fn hf_preupload(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((kind_plural, owner, name, revision)): Path<(String, String, String, String)>,
    Json(input): Json<HfPreuploadInput>,
) -> AppResult<Json<serde_json::Value>> {
    user.require_scope("write")?;
    let kind = hf_kind(&kind_plural)?;
    if revision != "main" {
        return Err(AppError::BadRequest(
            "Hugging Face-compatible preupload currently supports only the main revision".into(),
        ));
    }
    ensure_repo_write_access(&state, &user, kind, &owner, &name).await?;
    for file in &input.files {
        validate_path(&file.path)?;
        if file.size > 10 * 1024 * 1024 {
            return Err(AppError::BadRequest(
                "Hugging Face-compatible uploads are limited to 10 MiB per file in v1; use the OpenHug CLI for larger files".into(),
            ));
        }
    }
    Ok(Json(
        serde_json::json!({"files": input.files.into_iter().map(|file| serde_json::json!({
        "path": file.path, "uploadMode": "regular", "shouldIgnore": false
    })).collect::<Vec<_>>() }),
    ))
}

pub async fn hf_commit(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((kind_plural, owner, name, revision)): Path<(String, String, String, String)>,
    body: Bytes,
) -> AppResult<impl IntoResponse> {
    user.require_scope("write")?;
    let kind = hf_kind(&kind_plural)?;
    if revision != "main" {
        return Err(AppError::BadRequest(
            "Hugging Face-compatible commits currently support only the main revision".into(),
        ));
    }
    ensure_repo_write_access(&state, &user, kind, &owner, &name).await?;
    let text = std::str::from_utf8(&body)
        .map_err(|_| AppError::BadRequest("commit payload must be UTF-8 NDJSON".into()))?;
    let mut message = "Commit through huggingface_hub".to_string();
    let mut files = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value = serde_json::from_str(line)
            .map_err(|_| AppError::BadRequest("invalid Hugging Face commit payload".into()))?;
        match value.get("key").and_then(|v| v.as_str()) {
            Some("header") => {
                if let Some(summary) = value.pointer("/value/summary").and_then(|v| v.as_str()) {
                    message = summary.to_string();
                }
            }
            Some("file") => {
                let path = value
                    .pointer("/value/path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AppError::BadRequest("file operation is missing path".into()))?;
                validate_path(path)?;
                let content = value
                    .pointer("/value/content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AppError::BadRequest("file operation is missing content".into())
                    })?;
                let bytes = STANDARD
                    .decode(content)
                    .map_err(|_| AppError::BadRequest("file content is not valid base64".into()))?;
                if bytes.len() > 10 * 1024 * 1024 {
                    return Err(AppError::BadRequest("inline file exceeds 10 MiB".into()));
                }
                let (sha256, size) = state
                    .storage
                    .put(Bytes::from(bytes))
                    .await
                    .map_err(AppError::Internal)?;
                sqlx::query("INSERT INTO blobs (sha256,size,storage_key) VALUES ($1,$2,$3) ON CONFLICT (sha256) DO NOTHING")
                    .bind(&sha256).bind(size).bind(blob_key(&sha256)).execute(&state.pool).await?;
                sqlx::query("INSERT INTO blob_uploads (user_id,sha256,size,expires_at) VALUES ($1,$2,$3,$4) ON CONFLICT (user_id,sha256) DO UPDATE SET size=excluded.size,expires_at=excluded.expires_at,created_at=now()")
                    .bind(user.id).bind(&sha256).bind(size).bind(Utc::now() + ChronoDuration::hours(24)).execute(&state.pool).await?;
                files.push(CommitFile {
                    path: path.into(),
                    sha256,
                    size,
                });
            }
            Some("deletedFile") | Some("deletedFolder") => {
                return Err(AppError::BadRequest(
                    "delete operations are not yet supported by the Hugging Face-compatible commit endpoint".into(),
                ));
            }
            _ => {
                return Err(AppError::BadRequest(
                    "unsupported Hugging Face commit operation".into(),
                ));
            }
        }
    }
    create_commit(
        State(state),
        user,
        Path((kind.into(), owner, name)),
        Json(CreateCommit {
            message,
            files,
            deletions: vec![],
        }),
    )
    .await
}

#[derive(Serialize, FromRow)]
pub struct InstanceSettings {
    instance_name: String,
    signup_policy: String,
    default_visibility: String,
    retention_days: i32,
}

pub async fn get_settings(
    State(state): State<AppState>,
    user: CurrentUser,
) -> AppResult<Json<InstanceSettings>> {
    if !user.is_superuser() {
        return Err(AppError::Forbidden);
    }
    user.require_scope("admin")?;
    Ok(Json(sqlx::query_as("SELECT instance_name,signup_policy,default_visibility::text AS default_visibility,retention_days FROM instance_settings").fetch_one(&state.pool).await?))
}

#[derive(Deserialize)]
pub struct UpdateSettings {
    instance_name: String,
    signup_policy: String,
    default_visibility: String,
    retention_days: i32,
}

pub async fn update_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(input): Json<UpdateSettings>,
) -> AppResult<Json<InstanceSettings>> {
    if !user.is_superuser() {
        return Err(AppError::Forbidden);
    }
    user.require_scope("admin")?;
    if input.instance_name.trim().is_empty()
        || input.instance_name.trim().len() > 80
        || !["disabled", "immediate", "approval"].contains(&input.signup_policy.as_str())
        || !["public", "private"].contains(&input.default_visibility.as_str())
        || !(1..=3650).contains(&input.retention_days)
    {
        return Err(AppError::BadRequest("invalid instance settings".into()));
    }
    Ok(Json(sqlx::query_as("UPDATE instance_settings SET instance_name=$1,signup_policy=$2,default_visibility=$3::text::repository_visibility,retention_days=$4 RETURNING instance_name,signup_policy,default_visibility::text AS default_visibility,retention_days")
        .bind(input.instance_name).bind(input.signup_policy).bind(input.default_visibility).bind(input.retention_days).fetch_one(&state.pool).await?))
}

#[derive(Serialize, FromRow)]
pub struct UserView {
    id: Uuid,
    username: String,
    email: String,
    role: String,
    status: String,
    created_at: DateTime<Utc>,
}
pub async fn list_users(
    State(state): State<AppState>,
    user: CurrentUser,
) -> AppResult<Json<Vec<UserView>>> {
    if !user.is_superuser() {
        return Err(AppError::Forbidden);
    }
    user.require_scope("admin")?;
    Ok(Json(sqlx::query_as("SELECT id,username,email,role::text AS role,status::text AS status,created_at FROM users ORDER BY created_at DESC").fetch_all(&state.pool).await?))
}
#[derive(Deserialize)]
pub struct UpdateUser {
    status: String,
    role: Option<String>,
}
pub async fn update_user(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateUser>,
) -> AppResult<StatusCode> {
    if !user.is_superuser() {
        return Err(AppError::Forbidden);
    }
    user.require_scope("admin")?;
    if id == user.id && input.status != "active" {
        return Err(AppError::BadRequest(
            "you cannot suspend your own account".into(),
        ));
    }
    if !["pending", "active", "suspended"].contains(&input.status.as_str())
        || input
            .role
            .as_deref()
            .is_some_and(|v| !["user", "superuser"].contains(&v))
    {
        return Err(AppError::BadRequest("invalid user status or role".into()));
    }
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(684621339)")
        .execute(&mut *tx)
        .await?;
    let current: Option<(String, String)> =
        sqlx::query_as("SELECT role::text,status::text FROM users WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((current_role, current_status)) = current else {
        return Err(AppError::NotFound);
    };
    let next_role = input.role.as_deref().unwrap_or(&current_role);
    if current_role == "superuser"
        && current_status == "active"
        && (input.status != "active" || next_role != "superuser")
    {
        let remaining: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM users WHERE id<>$1 AND role='superuser' AND status='active'",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if remaining == 0 {
            return Err(AppError::BadRequest(
                "at least one active superuser must remain".into(),
            ));
        }
    }
    sqlx::query("UPDATE users SET status=$1::text::user_status,role=COALESCE($2::text::user_role,role) WHERE id=$3").bind(input.status).bind(input.role).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_repository(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((kind, owner, name)): Path<(String, String, String)>,
) -> AppResult<StatusCode> {
    user.require_scope("write")?;
    let result=sqlx::query("UPDATE repositories r SET deleted_at=now() FROM users u WHERE r.owner_id=u.id AND r.kind::text=$1 AND u.username=$2 AND r.name=$3 AND r.deleted_at IS NULL AND (r.owner_id=$4 OR $5)")
        .bind(kind).bind(owner).bind(name).bind(user.id).bind(user.is_superuser()).execute(&state.pool).await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

fn authorize_setup(
    state: &AppState,
    remote_addr: Option<SocketAddr>,
    submitted_token: Option<&str>,
) -> AppResult<()> {
    if let Some(expected_token) = &state.config.setup_token {
        if submitted_token
            .map(|token| {
                constant_time_eq(
                    hash_secret(token).as_bytes(),
                    hash_secret(expected_token).as_bytes(),
                )
            })
            .unwrap_or(false)
        {
            return Ok(());
        }
        return Err(AppError::Forbidden);
    }
    if remote_addr
        .map(|addr| addr.ip().is_loopback())
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0, |acc, (a, b)| acc | (a ^ b))
        == 0
}

fn check_login_throttle(remote_addr: Option<SocketAddr>, identity: &str) -> AppResult<()> {
    let now = Instant::now();
    let ip_key = remote_addr
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".into());
    let identity_key = identity.trim().to_lowercase();
    let keys = [format!("ip:{ip_key}"), format!("identity:{identity_key}")];
    let mut attempts = LOGIN_ATTEMPTS.lock().expect("login throttle lock poisoned");
    for key in &keys {
        let bucket = attempts.entry(key.clone()).or_default();
        while bucket
            .front()
            .is_some_and(|instant| now.duration_since(*instant) > LOGIN_WINDOW)
        {
            bucket.pop_front();
        }
        if bucket.len() >= MAX_LOGIN_ATTEMPTS_PER_WINDOW {
            return Err(AppError::RateLimited(
                "too many login attempts; try again shortly".into(),
            ));
        }
    }
    for key in &keys {
        attempts.entry(key.clone()).or_default().push_back(now);
    }
    Ok(())
}

fn hf_kind(kind_plural: &str) -> AppResult<&'static str> {
    match kind_plural {
        "models" => Ok("model"),
        "datasets" => Ok("dataset"),
        _ => Err(AppError::NotFound),
    }
}

async fn ensure_repo_write_access(
    state: &AppState,
    user: &CurrentUser,
    kind: &str,
    owner: &str,
    name: &str,
) -> AppResult<Uuid> {
    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT r.id,r.owner_id FROM repositories r JOIN users u ON u.id=r.owner_id WHERE r.kind::text=$1 AND u.username=$2 AND r.name=$3 AND r.deleted_at IS NULL",
    )
    .bind(kind)
    .bind(owner)
    .bind(name)
    .fetch_optional(&state.pool)
    .await?;
    let (repo_id, owner_id) = row.ok_or(AppError::NotFound)?;
    if owner_id != user.id && !user.is_superuser() {
        return Err(AppError::Forbidden);
    }
    Ok(repo_id)
}

fn download_headers(path: &str, commit: Uuid) -> AppResult<HeaderMap> {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let content_type = if is_active_content(mime.as_ref()) {
        "application/octet-stream"
    } else {
        mime.as_ref()
    };
    let filename = path.rsplit('/').next().unwrap_or("download");
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename*=UTF-8''{}",
            percent_encode_header(filename)
        ))
        .unwrap_or(HeaderValue::from_static("attachment")),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static(
            "sandbox; default-src 'none'; base-uri 'none'; form-action 'none'",
        ),
    );
    headers.insert(
        "x-repo-commit",
        HeaderValue::from_str(&commit.to_string()).unwrap(),
    );
    Ok(headers)
}

fn is_active_content(mime: &str) -> bool {
    matches!(
        mime,
        "text/html"
            | "image/svg+xml"
            | "application/xhtml+xml"
            | "application/xml"
            | "text/xml"
            | "text/javascript"
            | "application/javascript"
    )
}

fn percent_encode_header(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn validate_username(v: &str) -> AppResult<()> {
    if v.len() < 3
        || v.len() > 40
        || !v
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || v.starts_with('-')
        || v.ends_with('-')
    {
        Err(AppError::BadRequest(
            "username must be 3–40 lowercase letters, numbers, or hyphens".into(),
        ))
    } else {
        Ok(())
    }
}
fn validate_password(v: &str) -> AppResult<()> {
    if v.len() < 12 {
        Err(AppError::BadRequest(
            "password must be at least 12 characters".into(),
        ))
    } else {
        Ok(())
    }
}
fn validate_repo_name(v: &str) -> AppResult<()> {
    if v.is_empty()
        || v.len() > 96
        || !v
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    {
        Err(AppError::BadRequest("invalid repository name".into()))
    } else {
        Ok(())
    }
}
fn validate_path(v: &str) -> AppResult<()> {
    if v.is_empty()
        || v.starts_with('/')
        || v.split('/').any(|p| p.is_empty() || p == ".." || p == ".")
    {
        Err(AppError::BadRequest(format!(
            "invalid repository path: {v}"
        )))
    } else {
        Ok(())
    }
}

#[allow(dead_code)]
async fn _transaction_marker(_: &mut Transaction<'_, Postgres>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_repository_paths() {
        assert!(validate_path("weights/model.safetensors").is_ok());
        assert!(validate_path("../secret").is_err());
        assert!(validate_path("/absolute").is_err());
        assert!(validate_path("double//slash").is_err());
    }

    #[test]
    fn validates_usernames_and_passwords() {
        assert!(validate_username("model-owner").is_ok());
        assert!(validate_username("Admin").is_err());
        assert!(validate_username("ab").is_err());
        assert!(validate_password("a-long-password").is_ok());
        assert!(validate_password("short").is_err());
    }

    #[test]
    fn database_schema_contains_no_infrastructure_configuration() {
        let migration = include_str!("../../../migrations/0001_initial.sql").to_lowercase();
        for forbidden in [
            "database_url",
            "storage_endpoint",
            "access_key",
            "secret_key",
            "connection_url",
            "local_storage_path",
        ] {
            assert!(
                !migration.contains(forbidden),
                "schema contains {forbidden}"
            );
        }
    }
}
