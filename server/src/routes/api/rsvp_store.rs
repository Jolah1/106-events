//! Recording RSVP responses against `guest_invites`, whatever channel they
//! arrive through. The public link, WhatsApp, and SMS all funnel here so the
//! state machine lives in one place and behaves identically for each.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::error::AppError;

/// Where a response came from. Serialized into `guest_invites.responded_via`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Link,
    Whatsapp,
    Sms,
    Dashboard,
}

impl Channel {
    fn as_str(self) -> &'static str {
        match self {
            Channel::Link => "link",
            Channel::Whatsapp => "whatsapp",
            Channel::Sms => "sms",
            Channel::Dashboard => "dashboard",
        }
    }
}

/// One part's response, as the guest expressed it on the public link.
pub struct PartResponse {
    pub sub_event_id: Uuid,
    pub attending: bool,
    /// How many are coming, including the guest. Ignored when declining.
    pub party_size: i32,
}

/// A guest, resolved from an RSVP token or a matched phone number, with the one
/// fact the state machine needs to bound a party size.
pub struct RsvpGuest {
    pub id: Uuid,
    pub event_id: Uuid,
    pub name: String,
    pub plus_ones: i32,
}

pub async fn guest_by_token(pool: &PgPool, token: Uuid) -> Result<Option<RsvpGuest>, AppError> {
    Ok(sqlx::query_as!(
        RsvpGuest,
        "SELECT id, event_id, name, plus_ones FROM guests WHERE rsvp_token = $1",
        token
    )
    .fetch_optional(pool)
    .await?)
}

/// Resolves an inbound sender to a guest. A phone number can belong to a guest
/// at more than one event (the same aunt invited to two weddings), and a bare
/// "1" can't say which. We pick the event happening soonest — upcoming events
/// ahead of past ones — which is almost always the one a reply is about. A
/// guest can always use their per-event link for the other.
pub async fn guest_by_phone(pool: &PgPool, phone: &str) -> Result<Option<RsvpGuest>, AppError> {
    Ok(sqlx::query_as!(
        RsvpGuest,
        r#"
        SELECT g.id, g.event_id, g.name, g.plus_ones
        FROM guests g
        JOIN sub_events se ON se.event_id = g.event_id
        WHERE g.phone = $1
        GROUP BY g.id
        ORDER BY (min(se.starts_at) >= now()) DESC, min(se.starts_at) ASC
        LIMIT 1
        "#,
        phone
    )
    .fetch_optional(pool)
    .await?)
}

/// Applies one part's response inside a transaction. Only touches a part the
/// guest is actually invited to (the WHERE matches an existing invite row), so
/// a forged or stale sub_event_id changes nothing and reports as much.
///
/// Returns whether a row was updated, so a link submission naming a part the
/// guest isn't invited to can be rejected rather than silently ignored.
async fn apply_part(
    tx: &mut Transaction<'_, Postgres>,
    guest_id: Uuid,
    part: &PartResponse,
    max_party: i32,
    via: Channel,
    at: DateTime<Utc>,
) -> Result<bool, AppError> {
    let (status, party_size) = if part.attending {
        // A confirmed part carries at least the guest themselves, and never
        // more than their allowance — the ceiling the DB can't check alone.
        ("confirmed", part.party_size.clamp(1, max_party))
    } else {
        ("declined", 0)
    };

    let updated = sqlx::query_scalar!(
        r#"
        UPDATE guest_invites
        SET rsvp_status = $3, party_size = $4, responded_at = $5, responded_via = $6
        WHERE guest_id = $1 AND sub_event_id = $2
        RETURNING 1 AS "one!"
        "#,
        guest_id,
        part.sub_event_id,
        status,
        party_size,
        at,
        via.as_str()
    )
    .fetch_optional(&mut **tx)
    .await?;

    Ok(updated.is_some())
}

/// Records a per-part response from the public link. Every part named must be
/// one the guest is invited to; otherwise the whole submission is rejected, so
/// a tampered form can't half-apply.
pub async fn record_link_response(
    pool: &PgPool,
    guest: &RsvpGuest,
    parts: &[PartResponse],
) -> Result<(), AppError> {
    let max_party = 1 + guest.plus_ones;
    let now = Utc::now();
    let mut tx = pool.begin().await?;
    for part in parts {
        if !apply_part(&mut tx, guest.id, part, max_party, Channel::Link, now).await? {
            return Err(AppError::validation(
                "that response refers to a part you're not invited to",
            ));
        }
    }
    tx.commit().await?;
    Ok(())
}

/// The whole-invitation answer a coarse channel (WhatsApp/SMS "1"/"2") gives:
/// it can't speak to individual parts, so it sets every part the guest is
/// invited to at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkResponse {
    Confirm,
    Decline,
}

/// Applies a coarse confirm/decline to every part the guest is invited to.
///
/// A blanket confirm assumes the guest brings their full allowance
/// (1 + plus_ones): the organizer already budgeted for it, and over-catering
/// is the safer error at a Nigerian wedding. The guest can dial it back on the
/// public link, which speaks per-part. Returns the number of parts affected.
pub async fn record_bulk_response(
    pool: &PgPool,
    guest: &RsvpGuest,
    response: BulkResponse,
    via: Channel,
) -> Result<u64, AppError> {
    let (status, party_size) = match response {
        BulkResponse::Confirm => ("confirmed", 1 + guest.plus_ones),
        BulkResponse::Decline => ("declined", 0),
    };
    let affected = sqlx::query!(
        "UPDATE guest_invites
         SET rsvp_status = $2, party_size = $3, responded_at = now(), responded_via = $4
         WHERE guest_id = $1",
        guest.id,
        status,
        party_size,
        via.as_str()
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected)
}
