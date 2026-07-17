use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts},
};
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, AppResult},
};

#[derive(Clone, Debug, FromRow, Serialize)]
pub struct CurrentUser {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String,
    pub status: String,
    pub theme: String,
    #[serde(skip_serializing)]
    pub scopes: Vec<String>,
}

impl CurrentUser {
    pub fn is_superuser(&self) -> bool {
        self.role == "superuser"
    }

    pub fn require_scope(&self, scope: &str) -> AppResult<()> {
        if self
            .scopes
            .iter()
            .any(|value| value == scope || value == "admin")
        {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        authenticate(
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
    }
}

pub async fn authenticate(
    pool: &PgPool,
    auth: Option<&str>,
    cookie: Option<&str>,
) -> AppResult<CurrentUser> {
    let (secret, token_auth) = if let Some(token) = auth.and_then(|v| v.strip_prefix("Bearer ")) {
        (token, true)
    } else if let Some(session) = cookie.and_then(|c| {
        c.split(';')
            .find_map(|p| p.trim().strip_prefix("openhug_session="))
    }) {
        (session, false)
    } else {
        return Err(AppError::Unauthorized);
    };
    let hash = hash_secret(secret);
    let user = if token_auth {
        let user = sqlx::query_as::<_, CurrentUser>("SELECT u.id, u.username, u.email, u.role::text AS role, u.status::text AS status, u.theme, t.scopes FROM users u JOIN api_tokens t ON t.user_id=u.id WHERE t.token_hash=$1 AND (t.expires_at IS NULL OR t.expires_at > now()) AND u.status='active'")
            .bind(&hash).fetch_optional(pool).await?
            ;
        if user.is_some() {
            sqlx::query("UPDATE api_tokens SET last_used_at=now() WHERE token_hash=$1")
                .bind(&hash)
                .execute(pool)
                .await?;
        }
        user
    } else {
        sqlx::query_as::<_, CurrentUser>("SELECT u.id, u.username, u.email, u.role::text AS role, u.status::text AS status, u.theme, CASE WHEN u.role='superuser' THEN ARRAY['read','write','admin']::text[] ELSE ARRAY['read','write']::text[] END AS scopes FROM users u JOIN sessions s ON s.user_id=u.id WHERE s.id_hash=$1 AND s.expires_at > now() AND u.status='active'")
            .bind(&hash).fetch_optional(pool).await?
    };
    user.ok_or(AppError::Unauthorized)
}

pub async fn create_session(pool: &PgPool, user_id: Uuid) -> AppResult<String> {
    let secret = random_secret("ohs_");
    sqlx::query("INSERT INTO sessions (id_hash, user_id, expires_at) VALUES ($1,$2,$3)")
        .bind(hash_secret(&secret))
        .bind(user_id)
        .bind(Utc::now() + Duration::days(30))
        .execute(pool)
        .await?;
    Ok(secret)
}

pub fn random_secret(prefix: &str) -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    format!("{prefix}{}", hex::encode(bytes))
}

pub fn hash_secret(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_random_and_only_hashes_are_stable() {
        let first = random_secret("oh_");
        let second = random_secret("oh_");
        assert!(first.starts_with("oh_"));
        assert_ne!(first, second);
        assert_eq!(hash_secret(&first), hash_secret(&first));
        assert_ne!(hash_secret(&first), first);
    }
}
