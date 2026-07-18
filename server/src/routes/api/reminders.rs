//! Managing an event's reminder ladder, and reporting what it has done.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth::CurrentUser, error::AppError, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/events/{id}/reminders",
            get(list_reminders).post(create_reminder),
        )
        .route("/reminders/{id}", delete(delete_reminder))
}

/// A rung on the ladder. `offsetMinutes` is how long before the event's first
/// part it fires; `sentCount` is how many guests it has already reached, which
/// is what tells an organizer whether a rung has fired yet.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderSchedule {
    pub id: Uuid,
    pub event_id: Uuid,
    pub offset_minutes: i32,
    pub enabled: bool,
    pub sent_count: i64,
    pub failed_count: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewReminder {
    pub offset_minutes: i32,
}

/// Two years out is past the point of a "reminder", and a rung must be before
/// the event, not at it.
const MAX_OFFSET_MINUTES: i32 = 60 * 24 * 730;

async fn list_reminders(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(event_id): Path<Uuid>,
) -> Result<Json<Vec<ReminderSchedule>>, AppError> {
    let rows = sqlx::query!(
        r#"
        SELECT rs.id, rs.event_id, rs.offset_minutes, rs.enabled, rs.created_at,
               count(s.id) FILTER (WHERE s.status = 'sent') AS "sent!",
               count(s.id) FILTER (WHERE s.status = 'failed') AS "failed!"
        FROM reminder_schedules rs
        LEFT JOIN reminder_sends s ON s.schedule_id = rs.id
        WHERE rs.event_id = $1
        GROUP BY rs.id
        ORDER BY rs.offset_minutes DESC
        "#,
        event_id
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| ReminderSchedule {
                id: r.id,
                event_id: r.event_id,
                offset_minutes: r.offset_minutes,
                enabled: r.enabled,
                sent_count: r.sent,
                failed_count: r.failed,
                created_at: r.created_at,
            })
            .collect(),
    ))
}

async fn create_reminder(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(event_id): Path<Uuid>,
    Json(body): Json<NewReminder>,
) -> Result<impl IntoResponse, AppError> {
    if body.offset_minutes <= 0 {
        return Err(AppError::validation("a reminder must be before the event"));
    }
    if body.offset_minutes > MAX_OFFSET_MINUTES {
        return Err(AppError::validation("that's more than two years before the event"));
    }

    // The event has to exist; otherwise a typo'd id silently creates a rung
    // that can never fire.
    let exists = sqlx::query_scalar!(r#"SELECT 1 AS "one!" FROM events WHERE id = $1"#, event_id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    let row = sqlx::query!(
        r#"
        INSERT INTO reminder_schedules (event_id, offset_minutes)
        VALUES ($1, $2)
        ON CONFLICT (event_id, offset_minutes) DO NOTHING
        RETURNING id, event_id, offset_minutes, enabled, created_at
        "#,
        event_id,
        body.offset_minutes
    )
    .fetch_optional(&state.pool)
    .await?;

    let Some(row) = row else {
        return Err(AppError::Conflict(
            "that reminder is already on the schedule".into(),
        ));
    };

    Ok((
        StatusCode::CREATED,
        Json(ReminderSchedule {
            id: row.id,
            event_id: row.event_id,
            offset_minutes: row.offset_minutes,
            enabled: row.enabled,
            sent_count: 0,
            failed_count: 0,
            created_at: row.created_at,
        }),
    ))
}

/// Removing a rung takes its send ledger with it (ON DELETE CASCADE). That's
/// intended: the rung no longer exists, so there's nothing to be idempotent
/// about. Re-adding the same offset later is a deliberate act, and a guest who
/// hasn't answered by then is due another nudge anyway.
async fn delete_reminder(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let deleted = sqlx::query!("DELETE FROM reminder_schedules WHERE id = $1", id)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
