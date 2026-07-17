use axum::{extract::FromRequestParts, http::request::Parts};
use axum_extra::extract::CookieJar;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::tokens::{self, NewToken},
    error::AppError,
    state::AppState,
};

pub const SESSION_COOKIE: &str = "session_106";

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: Uuid,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// Extractor for authenticated API routes. Rejects with 401 when the session
/// cookie is missing, unknown, or expired.
pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let jar = CookieJar::from_headers(&parts.headers);
        let raw = jar
            .get(SESSION_COOKIE)
            .map(|c| c.value().to_string())
            .ok_or(AppError::Unauthorized)?;
        let hash = tokens::hash_token(&raw);

        let user = sqlx::query_as!(
            User,
            r#"
            SELECT u.id, u.email, u.phone, u.name, u.created_at
            FROM sessions s
            JOIN users u ON u.id = s.user_id
            WHERE s.token_hash = $1 AND s.expires_at > now()
            "#,
            hash
        )
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::Unauthorized)?;

        // Best-effort activity tracking; throttled to avoid a write per request.
        let _ = sqlx::query!(
            "UPDATE sessions SET last_seen_at = now()
             WHERE token_hash = $1 AND last_seen_at < now() - interval '15 minutes'",
            hash
        )
        .execute(&state.pool)
        .await;

        Ok(CurrentUser(user))
    }
}

/// Creates a session row and returns the raw token for the cookie.
pub async fn create_session(pool: &PgPool, user_id: Uuid, ttl_days: i64) -> Result<String, AppError> {
    let NewToken { raw, hash } = tokens::generate_token();
    sqlx::query!(
        "INSERT INTO sessions (user_id, token_hash, expires_at)
         VALUES ($1, $2, now() + make_interval(days => $3::int))",
        user_id,
        hash,
        ttl_days as i32
    )
    .execute(pool)
    .await?;
    Ok(raw)
}

pub async fn delete_session(pool: &PgPool, raw_token: &str) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM sessions WHERE token_hash = $1",
        tokens::hash_token(raw_token)
    )
    .execute(pool)
    .await?;
    Ok(())
}
