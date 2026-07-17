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
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    domain::{csv_import, phone, slug::slugify},
    error::AppError,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/events/{id}/guests", get(list_guests).post(create_guest))
        .route("/events/{id}/guests/import", post(import_guests))
        .route("/guests/{id}", patch(update_guest).delete(delete_guest))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Guest {
    pub id: Uuid,
    pub event_id: Uuid,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub plus_ones: i32,
    pub dietary: String,
    pub notes: String,
    /// The parts of the event this guest is invited to.
    pub sub_event_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const MAX_PLUS_ONES: i32 = 20;

fn validate_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 200 {
        return Err(AppError::validation("a guest needs a name of 1-200 characters"));
    }
    Ok(name.to_string())
}

/// Normalizes a phone number for storage, or clears it when blank. Rejecting
/// here rather than storing what was typed is what lets the RSVP phase match
/// an inbound WhatsApp sender to a row in this table.
fn validate_phone(raw: &str) -> Result<Option<String>, AppError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    phone::normalize(raw)
        .map(Some)
        .ok_or_else(|| AppError::validation(format!("{raw:?} isn't a phone number we can send to")))
}

fn validate_email(raw: &str) -> Result<Option<String>, AppError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    if !csv_import::is_emailish(raw) {
        return Err(AppError::validation(format!("{raw:?} isn't an email address")));
    }
    Ok(Some(raw.to_lowercase()))
}

fn validate_plus_ones(n: i32) -> Result<(), AppError> {
    if !(0..=MAX_PLUS_ONES).contains(&n) {
        return Err(AppError::validation(format!(
            "plus-ones must be between 0 and {MAX_PLUS_ONES}"
        )));
    }
    Ok(())
}

/// Maps the unique indexes onto messages an organizer can act on. Without
/// this a duplicate import surfaces as an opaque 500.
fn map_duplicate(err: sqlx::Error) -> AppError {
    match &err {
        sqlx::Error::Database(db) => match db.constraint() {
            Some("guests_by_event_phone") => {
                AppError::Conflict("another guest on this event already has that phone number".into())
            }
            Some("guests_by_event_email") => {
                AppError::Conflict("another guest on this event already has that email address".into())
            }
            _ => err.into(),
        },
        _ => err.into(),
    }
}

/// Confirms an event exists. Every staff member works the whole workspace, so
/// this only guards against a bad id, not against another organizer.
async fn fetch_event_id(pool: &PgPool, event_id: Uuid) -> Result<Uuid, AppError> {
    sqlx::query_scalar!("SELECT id FROM events WHERE id = $1", event_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

/// Resolves a guest id to its event.
async fn fetch_guest_event(pool: &PgPool, guest_id: Uuid) -> Result<Uuid, AppError> {
    sqlx::query_scalar!("SELECT event_id FROM guests WHERE id = $1", guest_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

async fn load_guests(pool: &PgPool, event_id: Uuid, only: Option<Uuid>) -> Result<Vec<Guest>, AppError> {
    let rows = sqlx::query!(
        r#"
        SELECT g.id, g.event_id, g.name, g.phone, g.email, g.plus_ones,
               g.dietary, g.notes, g.created_at, g.updated_at,
               COALESCE(
                   array_agg(gi.sub_event_id ORDER BY se.position)
                       FILTER (WHERE gi.sub_event_id IS NOT NULL),
                   '{}'
               ) AS "sub_event_ids!: Vec<Uuid>"
        FROM guests g
        LEFT JOIN guest_invites gi ON gi.guest_id = g.id
        LEFT JOIN sub_events se ON se.id = gi.sub_event_id
        WHERE g.event_id = $1 AND ($2::uuid IS NULL OR g.id = $2)
        GROUP BY g.id
        ORDER BY g.name, g.created_at
        "#,
        event_id,
        only
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Guest {
            id: r.id,
            event_id: r.event_id,
            name: r.name,
            phone: r.phone,
            email: r.email,
            plus_ones: r.plus_ones,
            dietary: r.dietary,
            notes: r.notes,
            sub_event_ids: r.sub_event_ids,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect())
}

async fn load_guest(pool: &PgPool, event_id: Uuid, guest_id: Uuid) -> Result<Guest, AppError> {
    load_guests(pool, event_id, Some(guest_id))
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

async fn list_guests(
    State(state): State<AppState>,
    CurrentUser(_user): CurrentUser,
    Path(event_id): Path<Uuid>,
) -> Result<Json<Vec<Guest>>, AppError> {
    fetch_event_id(&state.pool, event_id).await?;
    Ok(Json(load_guests(&state.pool, event_id, None).await?))
}

/// Replaces a guest's invitations with exactly `sub_event_ids`. The composite
/// foreign key on guest_invites rejects a sub-event belonging to another
/// event, so this cannot leak across events even if the ids are forged.
async fn set_invites(
    tx: &mut Transaction<'_, Postgres>,
    event_id: Uuid,
    guest_id: Uuid,
    sub_event_ids: &[Uuid],
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM guest_invites WHERE guest_id = $1 AND NOT (sub_event_id = ANY($2))",
        guest_id,
        sub_event_ids
    )
    .execute(&mut **tx)
    .await?;
    add_invites(tx, event_id, guest_id, sub_event_ids).await
}

/// Adds invitations, leaving existing ones alone.
async fn add_invites(
    tx: &mut Transaction<'_, Postgres>,
    event_id: Uuid,
    guest_id: Uuid,
    sub_event_ids: &[Uuid],
) -> Result<(), AppError> {
    if sub_event_ids.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "INSERT INTO guest_invites (guest_id, sub_event_id, event_id)
         SELECT $1, unnest($2::uuid[]), $3
         ON CONFLICT DO NOTHING",
        guest_id,
        sub_event_ids,
        event_id
    )
    .execute(&mut **tx)
    .await
    .map_err(|err| match &err {
        // The composite FK fires when an id isn't a sub-event of this event —
        // which from the caller's side is simply an id that doesn't exist.
        sqlx::Error::Database(db) if db.constraint().is_some_and(|c| c.starts_with("guest_invites_sub_event_id")) => {
            AppError::validation("one of those parts doesn't belong to this event")
        }
        _ => err.into(),
    })?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateGuestBody {
    pub name: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub plus_ones: i32,
    #[serde(default)]
    pub dietary: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub sub_event_ids: Vec<Uuid>,
}

async fn create_guest(
    State(state): State<AppState>,
    CurrentUser(_user): CurrentUser,
    Path(event_id): Path<Uuid>,
    Json(body): Json<CreateGuestBody>,
) -> Result<impl IntoResponse, AppError> {
    fetch_event_id(&state.pool, event_id).await?;
    let name = validate_name(&body.name)?;
    let phone = validate_phone(&body.phone)?;
    let email = validate_email(&body.email)?;
    validate_plus_ones(body.plus_ones)?;

    let mut tx = state.pool.begin().await?;
    let guest_id = sqlx::query_scalar!(
        "INSERT INTO guests (event_id, name, phone, email, plus_ones, dietary, notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
        event_id,
        name,
        phone,
        email,
        body.plus_ones,
        body.dietary.trim(),
        body.notes.trim()
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(map_duplicate)?;

    add_invites(&mut tx, event_id, guest_id, &body.sub_event_ids).await?;
    tx.commit().await?;

    let guest = load_guest(&state.pool, event_id, guest_id).await?;
    Ok((StatusCode::CREATED, Json(guest)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGuestBody {
    pub name: Option<String>,
    /// Absent = keep, null or "" = clear, value = set.
    #[serde(default, deserialize_with = "double_option")]
    pub phone: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub email: Option<Option<String>>,
    pub plus_ones: Option<i32>,
    pub dietary: Option<String>,
    pub notes: Option<String>,
    /// When present, replaces the guest's invitations wholesale.
    pub sub_event_ids: Option<Vec<Uuid>>,
}

fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Folds the "absent / clear / set" triple into the (should_update, value)
/// pair the UPDATE statements below take. An empty string clears, matching
/// how the cover-image field behaves on events.
fn resolve_optional_text(
    field: Option<Option<String>>,
    validate: impl Fn(&str) -> Result<Option<String>, AppError>,
) -> Result<(bool, Option<String>), AppError> {
    match field {
        None => Ok((false, None)),
        Some(None) => Ok((true, None)),
        Some(Some(raw)) => Ok((true, validate(&raw)?)),
    }
}

async fn update_guest(
    State(state): State<AppState>,
    CurrentUser(_user): CurrentUser,
    Path(guest_id): Path<Uuid>,
    Json(body): Json<UpdateGuestBody>,
) -> Result<Json<Guest>, AppError> {
    let event_id = fetch_guest_event(&state.pool, guest_id).await?;

    let name = body.name.as_deref().map(validate_name).transpose()?;
    let (set_phone, phone) = resolve_optional_text(body.phone, validate_phone)?;
    let (set_email, email) = resolve_optional_text(body.email, validate_email)?;
    if let Some(n) = body.plus_ones {
        validate_plus_ones(n)?;
    }

    let mut tx = state.pool.begin().await?;
    sqlx::query!(
        r#"
        UPDATE guests SET
            name = COALESCE($2, name),
            phone = CASE WHEN $3 THEN $4 ELSE phone END,
            email = CASE WHEN $5 THEN $6 ELSE email END,
            plus_ones = COALESCE($7, plus_ones),
            dietary = COALESCE($8, dietary),
            notes = COALESCE($9, notes),
            updated_at = now()
        WHERE id = $1
        "#,
        guest_id,
        name,
        set_phone,
        phone,
        set_email,
        email,
        body.plus_ones,
        body.dietary.as_deref().map(str::trim),
        body.notes.as_deref().map(str::trim)
    )
    .execute(&mut *tx)
    .await
    .map_err(map_duplicate)?;

    if let Some(sub_event_ids) = &body.sub_event_ids {
        set_invites(&mut tx, event_id, guest_id, sub_event_ids).await?;
    }
    tx.commit().await?;

    Ok(Json(load_guest(&state.pool, event_id, guest_id).await?))
}

async fn delete_guest(
    State(state): State<AppState>,
    CurrentUser(_user): CurrentUser,
    Path(guest_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    fetch_guest_event(&state.pool, guest_id).await?;
    sqlx::query!("DELETE FROM guests WHERE id = $1", guest_id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBody {
    /// The file's contents. UTF-8: Excel's "CSV UTF-8" export, or anything
    /// Google Sheets produces.
    pub csv: String,
    /// Parts to invite every imported guest to. A row's own parts column, if
    /// the file has one, wins over this.
    #[serde(default)]
    pub sub_event_ids: Vec<Uuid>,
    /// Parse and report without writing. The dashboard runs this first so the
    /// organizer sees the damage before committing to it.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub dry_run: bool,
    pub created: usize,
    pub updated: usize,
    /// Rows that would not be imported, with the file line to look at.
    pub errors: Vec<ImportRowError>,
    pub ignored_columns: Vec<String>,
    /// Part names in the file that don't match any part of this event.
    pub unknown_parts: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRowError {
    pub line: u64,
    pub message: String,
}

impl From<csv_import::RowError> for ImportRowError {
    fn from(err: csv_import::RowError) -> Self {
        Self { line: err.line, message: err.message }
    }
}

impl From<csv_import::CsvError> for AppError {
    fn from(err: csv_import::CsvError) -> Self {
        AppError::validation(err.to_string())
    }
}

/// Finds the guest an imported row refers to, matching on phone first and then
/// email — the same keys the unique indexes enforce.
///
/// Returning two ids means the row's phone and email each already belong to a
/// *different* guest. Merging them is a judgement call we shouldn't make on
/// the organizer's behalf, and writing it anyway would just trip a unique
/// index and abort the whole import.
async fn match_existing_guest(
    tx: &mut Transaction<'_, Postgres>,
    event_id: Uuid,
    phone: Option<&str>,
    email: Option<&str>,
) -> Result<Result<Option<Uuid>, &'static str>, AppError> {
    if phone.is_none() && email.is_none() {
        return Ok(Ok(None));
    }
    let ids = sqlx::query_scalar!(
        "SELECT id FROM guests
         WHERE event_id = $1
           AND (($2::text IS NOT NULL AND phone = $2)
             OR ($3::text IS NOT NULL AND email = $3))
         FOR UPDATE",
        event_id,
        phone,
        email
    )
    .fetch_all(&mut **tx)
    .await?;

    Ok(match ids.len() {
        0 => Ok(None),
        1 => Ok(Some(ids[0])),
        _ => Err("this row's phone and email belong to two different guests already"),
    })
}

/// Indexes an event's parts by slug so a CSV can name them however it likes:
/// "Church Ceremony", "church-ceremony" and "CHURCH CEREMONY" all resolve.
async fn part_index(pool: &PgPool, event_id: Uuid) -> Result<HashMap<String, Uuid>, AppError> {
    let parts = sqlx::query!("SELECT id, name, slug FROM sub_events WHERE event_id = $1", event_id)
        .fetch_all(pool)
        .await?;
    let mut index = HashMap::new();
    for part in parts {
        index.insert(part.slug, part.id);
        index.insert(slugify(&part.name), part.id);
    }
    Ok(index)
}

async fn import_guests(
    State(state): State<AppState>,
    CurrentUser(_user): CurrentUser,
    Path(event_id): Path<Uuid>,
    Json(body): Json<ImportBody>,
) -> Result<Json<ImportReport>, AppError> {
    fetch_event_id(&state.pool, event_id).await?;
    let parsed = csv_import::parse(&body.csv)?;
    let parts = part_index(&state.pool, event_id).await?;

    let mut report = ImportReport {
        dry_run: body.dry_run,
        created: 0,
        updated: 0,
        errors: parsed.errors.into_iter().map(Into::into).collect(),
        ignored_columns: parsed.ignored_columns,
        unknown_parts: Vec::new(),
    };

    let mut tx = state.pool.begin().await?;

    for row in &parsed.rows {
        // Resolve the row's own parts column, if it had one, else fall back to
        // the parts chosen in the dashboard for this whole import.
        let mut invites = Vec::new();
        let mut row_failed = false;
        for name in &row.parts {
            match parts.get(&slugify(name)) {
                Some(id) => invites.push(*id),
                None => {
                    if !report.unknown_parts.iter().any(|p| p == name) {
                        report.unknown_parts.push(name.clone());
                    }
                    report.errors.push(ImportRowError {
                        line: row.line,
                        message: format!("{name:?} isn't a part of this event"),
                    });
                    row_failed = true;
                    break;
                }
            }
        }
        if row_failed {
            continue;
        }
        if invites.is_empty() {
            invites = body.sub_event_ids.clone();
        }

        let existing = match match_existing_guest(
            &mut tx,
            event_id,
            row.phone.as_deref(),
            row.email.as_deref(),
        )
        .await?
        {
            Ok(existing) => existing,
            Err(message) => {
                report.errors.push(ImportRowError { line: row.line, message: message.into() });
                continue;
            }
        };

        let guest_id = match existing {
            Some(guest_id) => {
                // Only overwrite what the file actually says. A spreadsheet
                // without a dietary column shouldn't erase notes typed into
                // the dashboard.
                sqlx::query!(
                    r#"
                    UPDATE guests SET
                        name = $2,
                        phone = COALESCE($3, phone),
                        email = COALESCE($4, email),
                        plus_ones = COALESCE($5, plus_ones),
                        dietary = CASE WHEN $6 = '' THEN dietary ELSE $6 END,
                        notes = CASE WHEN $7 = '' THEN notes ELSE $7 END,
                        updated_at = now()
                    WHERE id = $1
                    "#,
                    guest_id,
                    row.name,
                    row.phone,
                    row.email,
                    row.plus_ones,
                    row.dietary,
                    row.notes
                )
                .execute(&mut *tx)
                .await
                .map_err(map_duplicate)?;
                report.updated += 1;
                guest_id
            }
            None => {
                let guest_id = sqlx::query_scalar!(
                    "INSERT INTO guests (event_id, name, phone, email, plus_ones, dietary, notes)
                     VALUES ($1, $2, $3, $4, COALESCE($5, 0), $6, $7)
                     RETURNING id",
                    event_id,
                    row.name,
                    row.phone,
                    row.email,
                    row.plus_ones,
                    row.dietary,
                    row.notes
                )
                .fetch_one(&mut *tx)
                .await
                .map_err(map_duplicate)?;
                report.created += 1;
                guest_id
            }
        };

        // Importing adds invitations and never removes them: organizers import
        // one list per part ("reception.csv", then "engagement.csv"), and the
        // second upload must not undo the first.
        add_invites(&mut tx, event_id, guest_id, &invites).await?;
    }

    if body.dry_run {
        tx.rollback().await?;
    } else {
        tx.commit().await?;
    }

    Ok(Json(report))
}
