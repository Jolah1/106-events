use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    domain::slug::{random_suffix, slugify},
    error::AppError,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/events", post(create_event).get(list_events))
        .route(
            "/events/{id}",
            get(get_event).patch(update_event).delete(delete_event),
        )
        .route("/events/{id}/sub-events", post(create_sub_event))
        .route(
            "/sub-events/{id}",
            patch(update_sub_event).delete(delete_sub_event),
        )
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: Uuid,
    pub title: String,
    pub slug: String,
    pub description: String,
    pub cover_image_url: Option<String>,
    pub timezone: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SubEvent {
    pub id: Uuid,
    pub event_id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
    pub venue_name: String,
    pub venue_address: String,
    pub is_default: bool,
    pub position: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventSummary {
    #[serde(flatten)]
    pub event: Event,
    pub sub_event_count: i64,
    pub first_starts_at: Option<DateTime<Utc>>,
    pub last_starts_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDetail {
    #[serde(flatten)]
    pub event: Event,
    pub sub_events: Vec<SubEvent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubEventInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub venue_name: String,
    #[serde(default)]
    pub venue_address: String,
    /// Marks the auto-created sub-event of a "simple" (single-part) event;
    /// the UI hides the sub-event layer when the only sub-event is default.
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEventBody {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub timezone: Option<String>,
    pub cover_image_url: Option<String>,
    pub sub_events: Vec<SubEventInput>,
}

fn validate_title(title: &str) -> Result<String, AppError> {
    let title = title.trim();
    if title.is_empty() || title.len() > 200 {
        return Err(AppError::validation("title must be 1-200 characters"));
    }
    Ok(title.to_string())
}

fn validate_timezone(tz: &str) -> Result<String, AppError> {
    tz.parse::<chrono_tz::Tz>()
        .map(|_| tz.to_string())
        .map_err(|_| AppError::validation(format!("unknown timezone {tz:?}")))
}

/// Cover images are rendered into `<img src>` and `og:image` on the public
/// page, so only absolute http(s) URLs are ever accepted. An empty string is
/// the documented "clear it" sentinel and passes through untouched.
fn validate_cover_image_url(url: &str) -> Result<(), AppError> {
    let url = url.trim();
    if url.is_empty() {
        return Ok(());
    }
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(AppError::validation(
            "cover image must be a full https:// or http:// URL",
        ));
    }
    if url.len() > 2000 {
        return Err(AppError::validation("cover image URL is too long"));
    }
    Ok(())
}

fn validate_sub_event_input(input: &SubEventInput) -> Result<(), AppError> {
    let name = input.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(AppError::validation("sub-event name must be 1-120 characters"));
    }
    if let Some(ends_at) = input.ends_at
        && ends_at <= input.starts_at
    {
        return Err(AppError::validation(format!("{name:?} must end after it starts")));
    }
    Ok(())
}

/// Inserts the event, retrying with a random slug suffix on collision.
/// Collisions are detected via ON CONFLICT DO NOTHING (no row returned) rather
/// than a unique-violation error, which would abort the enclosing transaction.
async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    body: &CreateEventBody,
    title: &str,
    timezone: &str,
) -> Result<Event, AppError> {
    let base = slugify(title);
    for attempt in 0..5 {
        let slug = match (attempt, base.is_empty()) {
            (0, false) => base.clone(),
            _ => {
                let base = if base.is_empty() { "event" } else { &base };
                format!("{base}-{}", random_suffix(4))
            }
        };
        let inserted = sqlx::query_as!(
            Event,
            "INSERT INTO events (user_id, title, slug, description, cover_image_url, timezone)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (slug) DO NOTHING
             RETURNING id, title, slug, description, cover_image_url, timezone, created_at, updated_at",
            user_id,
            title,
            slug,
            body.description.trim(),
            body.cover_image_url.as_deref().filter(|u| !u.trim().is_empty()),
            timezone
        )
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(event) = inserted {
            return Ok(event);
        }
    }
    Err(AppError::Internal(anyhow::anyhow!(
        "could not find a free slug after 5 attempts"
    )))
}

/// Inserts a sub-event, retrying with a random slug suffix on collision
/// within the event.
async fn insert_sub_event(
    tx: &mut Transaction<'_, Postgres>,
    event_id: Uuid,
    input: &SubEventInput,
    position: i32,
) -> Result<SubEvent, AppError> {
    validate_sub_event_input(input)?;
    let name = input.name.trim();
    let base = slugify(name);
    for attempt in 0..5 {
        let slug = match (attempt, base.is_empty()) {
            (0, false) => base.clone(),
            _ => {
                let base = if base.is_empty() { "part" } else { &base };
                format!("{base}-{}", random_suffix(4))
            }
        };
        let inserted = sqlx::query_as!(
            SubEvent,
            "INSERT INTO sub_events
                 (event_id, name, slug, description, starts_at, ends_at,
                  venue_name, venue_address, is_default, position)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (event_id, slug) DO NOTHING
             RETURNING id, event_id, name, slug, description, starts_at, ends_at,
                       venue_name, venue_address, is_default, position",
            event_id,
            name,
            slug,
            input.description.trim(),
            input.starts_at,
            input.ends_at,
            input.venue_name.trim(),
            input.venue_address.trim(),
            input.is_default,
            position
        )
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(sub_event) = inserted {
            return Ok(sub_event);
        }
    }
    Err(AppError::Internal(anyhow::anyhow!(
        "could not find a free sub-event slug after 5 attempts"
    )))
}

async fn create_event(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<CreateEventBody>,
) -> Result<impl IntoResponse, AppError> {
    let title = validate_title(&body.title)?;
    let timezone = validate_timezone(body.timezone.as_deref().unwrap_or("Africa/Lagos"))?;
    if let Some(url) = &body.cover_image_url {
        validate_cover_image_url(url)?;
    }
    if body.sub_events.is_empty() || body.sub_events.len() > 20 {
        return Err(AppError::validation("an event needs 1-20 sub-events"));
    }
    if body.sub_events.iter().filter(|s| s.is_default).count() > 1 {
        return Err(AppError::validation("only one sub-event can be the default"));
    }

    let mut tx = state.pool.begin().await?;
    let event = insert_event(&mut tx, user.id, &body, &title, &timezone).await?;
    let mut sub_events = Vec::with_capacity(body.sub_events.len());
    for (idx, input) in body.sub_events.iter().enumerate() {
        sub_events.push(insert_sub_event(&mut tx, event.id, input, idx as i32).await?);
    }
    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(EventDetail { event, sub_events })))
}

async fn list_events(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<EventSummary>>, AppError> {
    let rows = sqlx::query!(
        r#"
        SELECT e.id, e.title, e.slug, e.description, e.cover_image_url, e.timezone,
               e.created_at, e.updated_at,
               count(se.id) AS "sub_event_count!",
               min(se.starts_at) AS first_starts_at,
               max(se.starts_at) AS last_starts_at
        FROM events e
        LEFT JOIN sub_events se ON se.event_id = e.id
        WHERE e.user_id = $1
        GROUP BY e.id
        ORDER BY min(se.starts_at) DESC NULLS LAST, e.created_at DESC
        "#,
        user.id
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| EventSummary {
                event: Event {
                    id: r.id,
                    title: r.title,
                    slug: r.slug,
                    description: r.description,
                    cover_image_url: r.cover_image_url,
                    timezone: r.timezone,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                },
                sub_event_count: r.sub_event_count,
                first_starts_at: r.first_starts_at,
                last_starts_at: r.last_starts_at,
            })
            .collect(),
    ))
}

async fn fetch_owned_event(
    pool: &PgPool,
    event_id: Uuid,
    user_id: Uuid,
) -> Result<Event, AppError> {
    sqlx::query_as!(
        Event,
        "SELECT id, title, slug, description, cover_image_url, timezone, created_at, updated_at
         FROM events WHERE id = $1 AND user_id = $2",
        event_id,
        user_id
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

async fn fetch_sub_events(pool: &PgPool, event_id: Uuid) -> Result<Vec<SubEvent>, AppError> {
    Ok(sqlx::query_as!(
        SubEvent,
        "SELECT id, event_id, name, slug, description, starts_at, ends_at,
                venue_name, venue_address, is_default, position
         FROM sub_events WHERE event_id = $1
         ORDER BY position, starts_at",
        event_id
    )
    .fetch_all(pool)
    .await?)
}

async fn get_event(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<EventDetail>, AppError> {
    let event = fetch_owned_event(&state.pool, id, user.id).await?;
    let sub_events = fetch_sub_events(&state.pool, id).await?;
    Ok(Json(EventDetail { event, sub_events }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEventBody {
    pub title: Option<String>,
    pub description: Option<String>,
    pub timezone: Option<String>,
    /// Empty string clears the cover image.
    pub cover_image_url: Option<String>,
}

async fn update_event(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateEventBody>,
) -> Result<Json<Event>, AppError> {
    let title = body.title.as_deref().map(validate_title).transpose()?;
    let timezone = body.timezone.as_deref().map(validate_timezone).transpose()?;
    if let Some(url) = &body.cover_image_url {
        validate_cover_image_url(url)?;
    }

    let event = sqlx::query_as!(
        Event,
        r#"
        UPDATE events SET
            title = COALESCE($3, title),
            description = COALESCE($4, description),
            timezone = COALESCE($5, timezone),
            cover_image_url = CASE
                WHEN $6::text IS NULL THEN cover_image_url
                WHEN $6 = '' THEN NULL
                ELSE $6
            END,
            updated_at = now()
        WHERE id = $1 AND user_id = $2
        RETURNING id, title, slug, description, cover_image_url, timezone, created_at, updated_at
        "#,
        id,
        user.id,
        title,
        body.description.as_deref().map(str::trim),
        timezone,
        body.cover_image_url.as_deref().map(str::trim)
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(event))
}

async fn delete_event(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    sqlx::query_scalar!(
        "DELETE FROM events WHERE id = $1 AND user_id = $2 RETURNING id",
        id,
        user.id
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_sub_event(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(event_id): Path<Uuid>,
    Json(input): Json<SubEventInput>,
) -> Result<impl IntoResponse, AppError> {
    fetch_owned_event(&state.pool, event_id, user.id).await?;

    let mut tx = state.pool.begin().await?;
    let position = sqlx::query_scalar!(
        r#"SELECT COALESCE(max(position), -1) + 1 AS "next!"
           FROM sub_events WHERE event_id = $1"#,
        event_id
    )
    .fetch_one(&mut *tx)
    .await?;
    let sub_event = insert_sub_event(&mut tx, event_id, &input, position).await?;
    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(sub_event)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSubEventBody {
    pub name: Option<String>,
    pub description: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    /// Absent = keep, null = clear, value = set. Plain `Option<Option<T>>`
    /// can't tell null from absent (serde folds null into the outer None),
    /// hence the custom deserializer.
    #[serde(default, deserialize_with = "double_option")]
    pub ends_at: Option<Option<DateTime<Utc>>>,
    pub venue_name: Option<String>,
    pub venue_address: Option<String>,
    pub position: Option<i32>,
}

fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

async fn update_sub_event(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateSubEventBody>,
) -> Result<Json<SubEvent>, AppError> {
    if let Some(name) = &body.name {
        let name = name.trim();
        if name.is_empty() || name.len() > 120 {
            return Err(AppError::validation("sub-event name must be 1-120 characters"));
        }
    }

    // ends_at uses a sentinel pair: $6 says "update it", $7 is the new value
    // (which may be NULL to clear). The DB CHECK still guards ordering.
    let (set_ends_at, ends_at) = match body.ends_at {
        None => (false, None),
        Some(value) => (true, value),
    };

    let sub_event = sqlx::query_as!(
        SubEvent,
        r#"
        UPDATE sub_events se SET
            name = COALESCE($3, se.name),
            description = COALESCE($4, se.description),
            starts_at = COALESCE($5, se.starts_at),
            ends_at = CASE WHEN $6 THEN $7 ELSE se.ends_at END,
            venue_name = COALESCE($8, se.venue_name),
            venue_address = COALESCE($9, se.venue_address),
            position = COALESCE($10, se.position),
            updated_at = now()
        FROM events e
        WHERE se.id = $1 AND e.id = se.event_id AND e.user_id = $2
        RETURNING se.id, se.event_id, se.name, se.slug, se.description, se.starts_at,
                  se.ends_at, se.venue_name, se.venue_address, se.is_default, se.position
        "#,
        id,
        user.id,
        body.name.as_deref().map(str::trim),
        body.description.as_deref().map(str::trim),
        body.starts_at,
        set_ends_at,
        ends_at,
        body.venue_name.as_deref().map(str::trim),
        body.venue_address.as_deref().map(str::trim),
        body.position
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|err| match &err {
        sqlx::Error::Database(db) if db.constraint() == Some("sub_events_check") => {
            AppError::validation("a sub-event must end after it starts")
        }
        _ => err.into(),
    })?
    .ok_or(AppError::NotFound)?;

    Ok(Json(sub_event))
}

async fn delete_sub_event(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.pool.begin().await?;
    // Lock the parent event row so concurrent deletes can't both pass the
    // "at least one sub-event remains" check.
    let event_id = sqlx::query_scalar!(
        "SELECT e.id FROM events e
         JOIN sub_events se ON se.event_id = e.id
         WHERE se.id = $1 AND e.user_id = $2
         FOR UPDATE OF e",
        id,
        user.id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;

    let remaining = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM sub_events WHERE event_id = $1"#,
        event_id
    )
    .fetch_one(&mut *tx)
    .await?;
    if remaining <= 1 {
        return Err(AppError::validation(
            "an event needs at least one sub-event; delete the event instead",
        ));
    }

    sqlx::query!("DELETE FROM sub_events WHERE id = $1", id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
