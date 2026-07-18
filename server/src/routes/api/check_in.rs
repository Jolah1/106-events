//! Attendance: issuing per-head codes and admitting people at the door.
//!
//! Attendance is free — the guest list is the only thing that grants entry, so
//! there is nothing to buy and nothing to reconcile against a payment. What
//! matters here is that a scan is *idempotent* and that a door with no signal
//! still works.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{auth::CurrentUser, domain::code, error::AppError, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/events/{id}/attendees", get(list_attendees))
        .route("/events/{id}/attendees/sync", post(sync_attendees))
        .route("/sub-events/{id}/check-in", post(check_in))
        .route("/guests/{id}/extra-head", post(extra_head))
        .route("/sub-events/{id}/check-ins", get(list_check_ins))
        .route("/sub-events/{id}/door", get(door_manifest))
}

/// One head, with the code that admits them.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attendee {
    pub id: Uuid,
    pub guest_id: Uuid,
    pub guest_name: String,
    /// "Aunt Ngozi" for head 0, "Aunt Ngozi +1" for the rest. Plus-ones have
    /// no names of their own, so this is the best label we can honestly give.
    pub label: String,
    pub head_index: i32,
    pub code: String,
    pub is_extra: bool,
}

fn label_for(guest_name: &str, head_index: i32) -> String {
    if head_index == 0 {
        guest_name.to_string()
    } else {
        format!("{guest_name} +{head_index}")
    }
}

/// Creates any missing attendee rows for a guest, up to `heads`.
///
/// Codes are issued once and never reissued: a guest who has already been sent
/// their QR must keep the same one, so raising a plus-one count adds codes
/// rather than rotating the existing ones.
async fn ensure_heads(
    tx: &mut Transaction<'_, Postgres>,
    guest_id: Uuid,
    event_id: Uuid,
    heads: i32,
) -> Result<u64, AppError> {
    let existing = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM attendees WHERE guest_id = $1 AND NOT is_extra"#,
        guest_id
    )
    .fetch_one(&mut **tx)
    .await?;

    // New heads take indexes above *every* existing one, extras included. A
    // door-added extra already occupies an index, and reusing it would collide
    // on (guest_id, head_index) the next time the guest's plus-ones go up.
    let next_index = sqlx::query_scalar!(
        r#"SELECT coalesce(max(head_index), -1) + 1 AS "next!" FROM attendees WHERE guest_id = $1"#,
        guest_id
    )
    .fetch_one(&mut **tx)
    .await?;

    let missing = (heads as i64 - existing).max(0);
    let mut created = 0;
    for offset in 0..missing {
        let head_index = next_index + offset as i32;
        // The unique index on code is the backstop; a retry on collision is
        // cheaper than coordinating uniqueness in application code.
        loop {
            let candidate = code::generate();
            let inserted = sqlx::query_scalar!(
                r#"
                INSERT INTO attendees (guest_id, event_id, head_index, code)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (code) DO NOTHING
                RETURNING 1 AS "one!"
                "#,
                guest_id,
                event_id,
                head_index,
                candidate
            )
            .fetch_optional(&mut **tx)
            .await?;
            if inserted.is_some() {
                created += 1;
                break;
            }
        }
    }
    Ok(created)
}

/// Issues a guest's codes outside the bulk sync.
///
/// Called the moment someone confirms, so their passes exist by the time the
/// redirect lands them back on the RSVP page. A guest should never be told
/// "come back later, your code isn't ready".
pub async fn ensure_heads_for_guest(
    pool: &PgPool,
    guest_id: Uuid,
    event_id: Uuid,
    heads: i32,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    ensure_heads(&mut tx, guest_id, event_id, heads).await?;
    tx.commit().await?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub created: u64,
    pub total: i64,
}

/// Brings the attendee list in line with the guest list.
///
/// Idempotent by construction: it only ever fills gaps, so running it after
/// every import is safe. It deliberately does *not* delete codes when a
/// plus-one count drops — that code may already be on someone's phone, and an
/// admitted guest turning into "invalid code" at the door is worse than an
/// unused row. The allowance check at check-in is what enforces the lower
/// number.
async fn sync_attendees(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(event_id): Path<Uuid>,
) -> Result<Json<SyncReport>, AppError> {
    let guests = sqlx::query!(
        "SELECT id, plus_ones FROM guests WHERE event_id = $1",
        event_id
    )
    .fetch_all(&state.pool)
    .await?;
    if guests.is_empty() {
        // Distinguishes an event with no guests from one that doesn't exist.
        sqlx::query_scalar!(r#"SELECT 1 AS "one!" FROM events WHERE id = $1"#, event_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or(AppError::NotFound)?;
    }

    let mut tx = state.pool.begin().await?;
    let mut created = 0;
    for guest in guests {
        created += ensure_heads(&mut tx, guest.id, event_id, 1 + guest.plus_ones).await?;
    }
    tx.commit().await?;

    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM attendees WHERE event_id = $1"#,
        event_id
    )
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(SyncReport { created, total }))
}

async fn list_attendees(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(event_id): Path<Uuid>,
) -> Result<Json<Vec<Attendee>>, AppError> {
    let rows = sqlx::query!(
        "SELECT a.id, a.guest_id, a.head_index, a.code, a.is_extra, g.name
         FROM attendees a JOIN guests g ON g.id = a.guest_id
         WHERE a.event_id = $1
         ORDER BY g.name, a.head_index",
        event_id
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| Attendee {
                label: label_for(&r.name, r.head_index),
                id: r.id,
                guest_id: r.guest_id,
                guest_name: r.name,
                head_index: r.head_index,
                code: r.code,
                is_extra: r.is_extra,
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckInRequest {
    /// The scanned or typed code.
    pub code: String,
    /// Set when staff deliberately admitted someone past their allowance.
    #[serde(default)]
    pub allow_over: bool,
    /// True when this scan happened offline and is being synced afterwards.
    #[serde(default)]
    pub offline: bool,
    /// When the scan actually happened. Absent means now. An offline queue
    /// replays scans minutes or hours later, and the door count should reflect
    /// when someone walked in, not when the signal came back.
    pub scanned_at: Option<DateTime<Utc>>,
}

/// What the door screen shows after a scan.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckInResult {
    /// "admitted", "already_in", "not_invited", "unknown_code", or
    /// "over_allowance" (a refusal the operator can retry with allowOver).
    pub outcome: String,
    pub label: Option<String>,
    pub guest_name: Option<String>,
    /// Heads already through for this guest at this part, including this one.
    pub party_checked_in: i64,
    /// What they confirmed for. Lets the door say "3 of 4".
    pub party_allowed: i32,
    pub checked_in_at: Option<DateTime<Utc>>,
}

impl CheckInResult {
    fn refusal(outcome: &str) -> Self {
        Self {
            outcome: outcome.into(),
            label: None,
            guest_name: None,
            party_checked_in: 0,
            party_allowed: 0,
            checked_in_at: None,
        }
    }
}

/// Admits one head to one part.
///
/// Every branch returns 200. A scanner is often a phone on a flaky connection
/// replaying a queue, and a non-2xx makes it retry — which for a check-in is
/// the one thing we don't want. The outcome carries the meaning instead.
async fn check_in(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(sub_event_id): Path<Uuid>,
    Json(body): Json<CheckInRequest>,
) -> Result<Json<CheckInResult>, AppError> {
    let scanned = code::normalize(&body.code);
    if !code::is_plausible(&scanned) {
        return Ok(Json(CheckInResult::refusal("unknown_code")));
    }

    let mut tx = state.pool.begin().await?;

    // Resolve the code, and lock the attendee row so two doors scanning the
    // same code at once serialize rather than both reading "not yet in".
    let attendee = sqlx::query!(
        "SELECT a.id, a.guest_id, a.event_id, a.head_index, g.name
         FROM attendees a JOIN guests g ON g.id = a.guest_id
         WHERE a.code = $1
         FOR UPDATE OF a",
        scanned
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(attendee) = attendee else {
        return Ok(Json(CheckInResult::refusal("unknown_code")));
    };

    // The invite for this part is what says they're expected and how many they
    // confirmed for. No invite means this code is for a different part of the
    // weekend — a real situation, and worth naming precisely.
    let invite = sqlx::query!(
        "SELECT rsvp_status, party_size FROM guest_invites
         WHERE guest_id = $1 AND sub_event_id = $2",
        attendee.guest_id,
        sub_event_id
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(invite) = invite else {
        let mut refusal = CheckInResult::refusal("not_invited");
        refusal.label = Some(label_for(&attendee.name, attendee.head_index));
        refusal.guest_name = Some(attendee.name);
        return Ok(Json(refusal));
    };

    // Someone who declined but turned up anyway is admitted on the same footing
    // as an over-allowance walk-in: staff decide, and it's recorded. A confirmed
    // party of 0 shouldn't silently mean "nobody may enter" when they're stood
    // at the door.
    let allowed = if invite.rsvp_status == "confirmed" {
        invite.party_size
    } else {
        0
    };

    let already_in = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM check_ins c
           JOIN attendees a ON a.id = c.attendee_id
           WHERE a.guest_id = $1 AND c.sub_event_id = $2"#,
        attendee.guest_id,
        sub_event_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let label = label_for(&attendee.name, attendee.head_index);

    // Already through: report it rather than counting them twice. This is the
    // common case for a replayed offline queue, not an error.
    let existing = sqlx::query!(
        "SELECT checked_in_at FROM check_ins WHERE attendee_id = $1 AND sub_event_id = $2",
        attendee.id,
        sub_event_id
    )
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(existing) = existing {
        return Ok(Json(CheckInResult {
            outcome: "already_in".into(),
            label: Some(label),
            guest_name: Some(attendee.name),
            party_checked_in: already_in,
            party_allowed: allowed,
            checked_in_at: Some(existing.checked_in_at),
        }));
    }

    let over = already_in >= allowed as i64;
    if over && !body.allow_over {
        // Not an error — a question for the operator, who can retry with
        // allowOver once they've decided.
        return Ok(Json(CheckInResult {
            outcome: "over_allowance".into(),
            label: Some(label),
            guest_name: Some(attendee.name),
            party_checked_in: already_in,
            party_allowed: allowed,
            checked_in_at: None,
        }));
    }

    let at = body.scanned_at.unwrap_or_else(Utc::now);
    let row = sqlx::query!(
        r#"
        INSERT INTO check_ins
            (attendee_id, sub_event_id, event_id, checked_in_at, checked_in_by,
             over_allowance, synced_offline)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (attendee_id, sub_event_id) DO NOTHING
        RETURNING checked_in_at
        "#,
        attendee.id,
        sub_event_id,
        attendee.event_id,
        at,
        user.0.id,
        over,
        body.offline
    )
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;

    // DO NOTHING means another connection won the race between our read and
    // our write. They're in either way, which is the outcome that matters.
    let Some(row) = row else {
        return Ok(Json(CheckInResult {
            outcome: "already_in".into(),
            label: Some(label),
            guest_name: Some(attendee.name),
            party_checked_in: already_in,
            party_allowed: allowed,
            checked_in_at: None,
        }));
    };

    Ok(Json(CheckInResult {
        outcome: "admitted".into(),
        label: Some(label),
        guest_name: Some(attendee.name),
        party_checked_in: already_in + 1,
        party_allowed: allowed,
        checked_in_at: Some(row.checked_in_at),
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckInRecord {
    pub attendee_id: Uuid,
    pub label: String,
    pub checked_in_at: DateTime<Utc>,
    pub over_allowance: bool,
    pub synced_offline: bool,
}

async fn list_check_ins(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(sub_event_id): Path<Uuid>,
) -> Result<Json<Vec<CheckInRecord>>, AppError> {
    let rows = sqlx::query!(
        "SELECT c.attendee_id, c.checked_in_at, c.over_allowance, c.synced_offline,
                a.head_index, g.name
         FROM check_ins c
         JOIN attendees a ON a.id = c.attendee_id
         JOIN guests g ON g.id = a.guest_id
         WHERE c.sub_event_id = $1
         ORDER BY c.checked_in_at DESC",
        sub_event_id
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| CheckInRecord {
                attendee_id: r.attendee_id,
                label: label_for(&r.name, r.head_index),
                checked_in_at: r.checked_in_at,
                over_allowance: r.over_allowance,
                synced_offline: r.synced_offline,
            })
            .collect(),
    ))
}

/// Everything the door needs to work with no signal at all.
///
/// The scanner downloads this once before doors open and then validates codes
/// against it locally. Without it an offline device can only queue blindly and
/// hope; with it, staff can tell an expected guest from a stranger while
/// completely disconnected.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoorManifest {
    pub sub_event_id: Uuid,
    pub sub_event_name: String,
    pub event_title: String,
    pub generated_at: DateTime<Utc>,
    pub entries: Vec<DoorEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoorEntry {
    pub code: String,
    pub label: String,
    pub guest_id: Uuid,
    /// Confirmed heads for this part; 0 when they declined or never answered.
    pub party_allowed: i32,
    /// Already through when the manifest was generated, so a device that syncs
    /// mid-event doesn't re-admit people.
    pub checked_in: bool,
}

async fn door_manifest(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(sub_event_id): Path<Uuid>,
) -> Result<Json<DoorManifest>, AppError> {
    let part = sqlx::query!(
        "SELECT s.name, s.event_id, e.title FROM sub_events s
         JOIN events e ON e.id = s.event_id WHERE s.id = $1",
        sub_event_id
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let rows = sqlx::query!(
        r#"
        SELECT a.code, a.head_index, a.guest_id, g.name,
               CASE WHEN gi.rsvp_status = 'confirmed' THEN gi.party_size ELSE 0 END
                   AS "party_allowed!",
               (c.id IS NOT NULL) AS "checked_in!"
        FROM attendees a
        JOIN guests g ON g.id = a.guest_id
        JOIN guest_invites gi ON gi.guest_id = a.guest_id AND gi.sub_event_id = $1
        LEFT JOIN check_ins c ON c.attendee_id = a.id AND c.sub_event_id = $1
        WHERE a.event_id = $2
        ORDER BY g.name, a.head_index
        "#,
        sub_event_id,
        part.event_id
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(DoorManifest {
        sub_event_id,
        sub_event_name: part.name,
        event_title: part.title,
        generated_at: Utc::now(),
        entries: rows
            .into_iter()
            .map(|r| DoorEntry {
                label: label_for(&r.name, r.head_index),
                code: r.code,
                guest_id: r.guest_id,
                party_allowed: r.party_allowed,
                checked_in: r.checked_in,
            })
            .collect(),
    }))
}

/// Adds a head beyond a guest's allowance, for someone who turned up alongside
/// a guest who *was* invited. Returns the new code so the door can admit them
/// on the spot. Flagged `is_extra`, so the organizer can see afterwards how
/// many people arrived that nobody had counted.
async fn extra_head(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(guest_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let (id, code) = add_extra_head(&state.pool, guest_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": id, "code": code })),
    ))
}

async fn add_extra_head(pool: &PgPool, guest_id: Uuid) -> Result<(Uuid, String), AppError> {
    let guest = sqlx::query!("SELECT event_id FROM guests WHERE id = $1", guest_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)?;

    let next_index = sqlx::query_scalar!(
        r#"SELECT coalesce(max(head_index), -1) + 1 AS "next!" FROM attendees WHERE guest_id = $1"#,
        guest_id
    )
    .fetch_one(pool)
    .await?;

    loop {
        let candidate = code::generate();
        let row = sqlx::query!(
            r#"
            INSERT INTO attendees (guest_id, event_id, head_index, code, is_extra)
            VALUES ($1, $2, $3, $4, TRUE)
            ON CONFLICT (code) DO NOTHING
            RETURNING id
            "#,
            guest_id,
            guest.event_id,
            next_index,
            candidate
        )
        .fetch_optional(pool)
        .await?;
        if let Some(row) = row {
            return Ok((row.id, candidate));
        }
    }
}
