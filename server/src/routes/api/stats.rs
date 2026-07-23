//! The organizer's rollup: one call that answers "where does this event
//! stand?" — who's replied, how many heads are coming to each part, and how
//! many actually walked through the door.
//!
//! Everything here is derived from tables other endpoints own. Nothing is
//! stored, so the numbers can't drift from the rows they summarize; a rollup
//! that disagrees with the guest list would be worse than no rollup at all.

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{auth::CurrentUser, error::AppError, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/events/{id}/stats", get(event_stats))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventStats {
    pub event_id: Uuid,
    /// Parties on the list, and the most heads they could bring (1 + plus-ones
    /// each). The ceiling the venue plans against, not a promise.
    pub guest_count: i64,
    pub heads_invited: i64,
    /// A guest has replied when every part they're invited to has an answer;
    /// one pending part keeps them in the awaiting bucket, because they still
    /// need chasing about it.
    pub replied_guests: i64,
    pub awaiting_guests: i64,
    pub vendor_count: i64,
    pub vendor_cost_kobo: i64,
    pub vendor_paid_kobo: i64,
    /// Clamped per vendor, like the vendor sheet's own strip, so one overpaid
    /// supplier can't hide another's debt.
    pub vendor_outstanding_kobo: i64,
    pub parts: Vec<PartStats>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartStats {
    pub sub_event_id: Uuid,
    pub name: String,
    pub starts_at: DateTime<Utc>,
    pub is_default: bool,
    /// Parties invited to this part, and how each stands.
    pub invited_parties: i64,
    pub confirmed_parties: i64,
    pub declined_parties: i64,
    pub pending_parties: i64,
    /// Heads confirmed parties said they'd bring — what catering cooks for.
    pub confirmed_heads: i64,
    /// Heads actually admitted, including walk-ins past the allowance.
    pub checked_in_heads: i64,
    pub over_allowance_heads: i64,
    /// Admitted while the door had no signal and synced later; tells the
    /// organizer a live count from one that caught up afterwards.
    pub offline_synced_heads: i64,
}

async fn event_stats(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(event_id): Path<Uuid>,
) -> Result<Json<EventStats>, AppError> {
    sqlx::query_scalar!(r#"SELECT 1 AS "one!" FROM events WHERE id = $1"#, event_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;

    let guests = sqlx::query!(
        r#"
        SELECT
            count(*) AS "guest_count!",
            COALESCE(sum(1 + g.plus_ones), 0)::bigint AS "heads_invited!",
            count(*) FILTER (WHERE
                EXISTS (SELECT 1 FROM guest_invites gi WHERE gi.guest_id = g.id)
                AND NOT EXISTS (SELECT 1 FROM guest_invites gi
                                WHERE gi.guest_id = g.id AND gi.rsvp_status = 'pending')
            ) AS "replied_guests!",
            count(*) FILTER (WHERE
                EXISTS (SELECT 1 FROM guest_invites gi
                        WHERE gi.guest_id = g.id AND gi.rsvp_status = 'pending')
            ) AS "awaiting_guests!"
        FROM guests g
        WHERE g.event_id = $1
        "#,
        event_id
    )
    .fetch_one(&state.pool)
    .await?;

    let vendors = sqlx::query!(
        r#"
        SELECT
            count(*) AS "vendor_count!",
            COALESCE(sum(cost_kobo), 0)::bigint AS "cost!",
            COALESCE(sum(amount_paid_kobo), 0)::bigint AS "paid!",
            COALESCE(sum(greatest(cost_kobo - amount_paid_kobo, 0)), 0)::bigint AS "outstanding!"
        FROM vendors
        WHERE event_id = $1
        "#,
        event_id
    )
    .fetch_one(&state.pool)
    .await?;

    // RSVP standings per part. FILTER over a LEFT JOIN: a part with no invites
    // contributes one all-NULL row, which no filter matches, so it lands as
    // zeros rather than vanishing from the rollup.
    let parts = sqlx::query!(
        r#"
        SELECT se.id, se.name, se.starts_at, se.is_default,
               count(gi.guest_id) AS "invited_parties!",
               count(*) FILTER (WHERE gi.rsvp_status = 'confirmed') AS "confirmed_parties!",
               count(*) FILTER (WHERE gi.rsvp_status = 'declined') AS "declined_parties!",
               count(*) FILTER (WHERE gi.rsvp_status = 'pending') AS "pending_parties!",
               COALESCE(sum(gi.party_size), 0)::bigint AS "confirmed_heads!"
        FROM sub_events se
        LEFT JOIN guest_invites gi ON gi.sub_event_id = se.id
        WHERE se.event_id = $1
        GROUP BY se.id
        ORDER BY se.position, se.starts_at
        "#,
        event_id
    )
    .fetch_all(&state.pool)
    .await?;

    let door = sqlx::query!(
        r#"
        SELECT sub_event_id,
               count(*) AS "heads!",
               count(*) FILTER (WHERE over_allowance) AS "over!",
               count(*) FILTER (WHERE synced_offline) AS "offline!"
        FROM check_ins
        WHERE event_id = $1
        GROUP BY sub_event_id
        "#,
        event_id
    )
    .fetch_all(&state.pool)
    .await?;
    let door: HashMap<Uuid, (i64, i64, i64)> = door
        .into_iter()
        .map(|r| (r.sub_event_id, (r.heads, r.over, r.offline)))
        .collect();

    let parts = parts
        .into_iter()
        .map(|p| {
            let (checked_in, over, offline) = door.get(&p.id).copied().unwrap_or((0, 0, 0));
            PartStats {
                sub_event_id: p.id,
                name: p.name,
                starts_at: p.starts_at,
                is_default: p.is_default,
                invited_parties: p.invited_parties,
                confirmed_parties: p.confirmed_parties,
                declined_parties: p.declined_parties,
                pending_parties: p.pending_parties,
                confirmed_heads: p.confirmed_heads,
                checked_in_heads: checked_in,
                over_allowance_heads: over,
                offline_synced_heads: offline,
            }
        })
        .collect();

    Ok(Json(EventStats {
        event_id,
        guest_count: guests.guest_count,
        heads_invited: guests.heads_invited,
        replied_guests: guests.replied_guests,
        awaiting_guests: guests.awaiting_guests,
        vendor_count: vendors.vendor_count,
        vendor_cost_kobo: vendors.cost,
        vendor_paid_kobo: vendors.paid,
        vendor_outstanding_kobo: vendors.outstanding,
        parts,
    }))
}
