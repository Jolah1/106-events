//! Managing who works at 106 Events.
//!
//! Staff are invited, never self-served (see `auth::verify`). This is where an
//! admin adds a colleague, changes a role, or removes someone who has left.
//! Adding a user *is* the invitation: their next magic link will sign them in.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{auth::CurrentUser, error::AppError, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/team", get(list_team).post(invite))
        .route("/team/{id}", post(update_member).delete(remove_member))
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub id: Uuid,
    pub email: Option<String>,
    pub name: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

/// Seeds the admins named in `ADMIN_EMAILS` at startup. Membership is otherwise
/// created only from inside the app, so without this first boot there would be
/// nobody who could sign in to invite anyone — a locked door with the key
/// inside. Idempotent: existing rows are promoted to admin, never duplicated,
/// and demoting an admin means removing them from the env var *and* changing
/// the role in-app (this won't undo that).
pub async fn seed_admins(pool: &PgPool, emails: &[String]) -> anyhow::Result<()> {
    for email in emails {
        sqlx::query!(
            "INSERT INTO users (email, role) VALUES ($1, 'admin')
             ON CONFLICT (email) DO UPDATE SET role = 'admin'",
            email
        )
        .execute(pool)
        .await?;
    }
    if !emails.is_empty() {
        tracing::info!("seeded {} admin(s) from ADMIN_EMAILS", emails.len());
    }
    Ok(())
}

fn validate_role(role: &str) -> Result<&str, AppError> {
    match role {
        "admin" | "staff" => Ok(role),
        _ => Err(AppError::validation("role must be 'admin' or 'staff'")),
    }
}

fn normalize_email(raw: &str) -> Result<String, AppError> {
    let email = raw.trim().to_lowercase();
    let valid = email.len() <= 254
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
        });
    if !valid {
        return Err(AppError::validation("enter a valid email address"));
    }
    Ok(email)
}

async fn list_team(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<Member>>, AppError> {
    user.require_admin()?;
    let members = sqlx::query_as!(
        Member,
        "SELECT id, email, name, role, created_at FROM users ORDER BY created_at",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(members))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteBody {
    pub email: String,
    #[serde(default)]
    pub name: String,
    /// Defaults to 'staff'; only an admin can create another admin.
    pub role: Option<String>,
}

async fn invite(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<InviteBody>,
) -> Result<(StatusCode, Json<Member>), AppError> {
    user.require_admin()?;
    let email = normalize_email(&body.email)?;
    let role = validate_role(body.role.as_deref().unwrap_or("staff"))?;
    let name = body.name.trim();
    if name.chars().count() > 200 {
        return Err(AppError::validation("name is too long"));
    }

    let member = sqlx::query_as!(
        Member,
        "INSERT INTO users (email, name, role, invited_by) VALUES ($1, $2, $3, $4)
         RETURNING id, email, name, role, created_at",
        email,
        name,
        role,
        user.id
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|err| match &err {
        sqlx::Error::Database(db) if db.constraint() == Some("users_email_key") => {
            AppError::Conflict("that email is already on the team".into())
        }
        _ => err.into(),
    })?;

    Ok((StatusCode::CREATED, Json(member)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMemberBody {
    pub role: Option<String>,
    pub name: Option<String>,
}

async fn update_member(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateMemberBody>,
) -> Result<Json<Member>, AppError> {
    user.require_admin()?;
    let role = body.role.as_deref().map(validate_role).transpose()?;

    // An admin must not be able to lock the team out by demoting the last
    // admin (themselves, most likely). Enforced under a lock so two concurrent
    // demotions can't both slip through.
    if role == Some("staff") {
        let mut tx = state.pool.begin().await?;
        let admins = sqlx::query_scalar!(
            // FOR UPDATE can't sit on an aggregate, so lock the admin rows in a
            // subquery and count them in the outer one.
            r#"SELECT count(*) AS "count!"
               FROM (SELECT 1 FROM users WHERE role = 'admin' FOR UPDATE) locked"#
        )
        .fetch_one(&mut *tx)
        .await?;
        let target_is_admin = sqlx::query_scalar!(
            "SELECT role = 'admin' FROM users WHERE id = $1",
            id
        )
        .fetch_optional(&mut *tx)
        .await?
        .flatten()
        .ok_or(AppError::NotFound)?;
        if admins <= 1 && target_is_admin {
            return Err(AppError::validation("the last admin can't be demoted"));
        }
        let member = update_member_row(&mut *tx, id, role, body.name.as_deref()).await?;
        tx.commit().await?;
        return Ok(Json(member));
    }

    let member = update_member_row(&state.pool, id, role, body.name.as_deref()).await?;
    Ok(Json(member))
}

async fn update_member_row(
    conn: impl sqlx::PgExecutor<'_>,
    id: Uuid,
    role: Option<&str>,
    name: Option<&str>,
) -> Result<Member, AppError> {
    sqlx::query_as!(
        Member,
        "UPDATE users SET role = COALESCE($2, role), name = COALESCE($3, name)
         WHERE id = $1
         RETURNING id, email, name, role, created_at",
        id,
        role,
        name.map(str::trim)
    )
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

async fn remove_member(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    user.require_admin()?;
    if id == user.id {
        return Err(AppError::validation("you can't remove yourself"));
    }

    // Removing the last admin would lock everyone out. Guard it the same way as
    // demotion, and treat the two paths as the one rule they are.
    let mut tx = state.pool.begin().await?;
    let admins = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!"
           FROM (SELECT 1 FROM users WHERE role = 'admin' FOR UPDATE) locked"#
    )
    .fetch_one(&mut *tx)
    .await?;
    let target = sqlx::query_scalar!("SELECT role FROM users WHERE id = $1", id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    if admins <= 1 && target == "admin" {
        return Err(AppError::validation("the last admin can't be removed"));
    }

    // Their events survive with created_by set to NULL (see migration 0004);
    // sessions cascade, signing them out everywhere.
    sqlx::query!("DELETE FROM users WHERE id = $1", id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
