//! The queue of people who've asked for an account.
//!
//! Reading and clearing it is an admin job — the same admins who invite staff,
//! since inviting someone *is* how a request gets resolved.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{auth::CurrentUser, domain::phone, error::AppError, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/access-requests", get(list))
        .route("/access-requests/{id}/handled", post(mark_handled))
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AccessRequest {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
    pub about: String,
    pub created_at: DateTime<Utc>,
    pub handled_at: Option<DateTime<Utc>>,
}

async fn list(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<AccessRequest>>, AppError> {
    user.require_admin()?;
    let requests = sqlx::query_as!(
        AccessRequest,
        "SELECT id, name, email, phone, about, created_at, handled_at
         FROM access_requests
         WHERE handled_at IS NULL
         ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(requests))
}

async fn mark_handled(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<AccessRequest>, AppError> {
    user.require_admin()?;
    let request = sqlx::query_as!(
        AccessRequest,
        "UPDATE access_requests SET handled_at = now(), handled_by = $2
         WHERE id = $1
         RETURNING id, name, email, phone, about, created_at, handled_at",
        id,
        user.id
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(Json(request))
}

/// Records a request from the landing page.
///
/// Asking again reopens the existing request and refreshes what they wrote,
/// rather than filling the queue with the same person twice. Someone who was
/// dismissed and asks again becomes open again — that's a person knocking
/// harder, which an admin should see.
pub async fn record(
    pool: &PgPool,
    name: &str,
    email: &str,
    raw_phone: &str,
    about: &str,
) -> Result<(), AppError> {
    let name = name.trim();
    let email = email.trim().to_lowercase();
    let about = about.trim();

    if name.is_empty() {
        return Err(AppError::validation("tell us your name"));
    }
    let valid_email = email.len() <= 254
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
        });
    if !valid_email {
        return Err(AppError::validation("enter a valid email address"));
    }
    if name.chars().count() > 200 {
        return Err(AppError::validation("that name is too long"));
    }
    if about.chars().count() > 2000 {
        return Err(AppError::validation("that message is too long"));
    }

    // The same E.164 normalization guests get, so a number here can be dialled
    // or messaged without anyone retyping it. An unparseable number is dropped
    // rather than refused: it's optional, and the email still reaches them.
    let phone = phone::normalize(raw_phone);

    sqlx::query!(
        "INSERT INTO access_requests (name, email, phone, about)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (lower(email)) DO UPDATE
         SET name = excluded.name,
             phone = coalesce(excluded.phone, access_requests.phone),
             about = excluded.about,
             created_at = now(),
             handled_at = NULL,
             handled_by = NULL",
        name,
        email,
        phone,
        about
    )
    .execute(pool)
    .await?;

    Ok(())
}
