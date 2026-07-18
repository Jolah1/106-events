//! Finding which reminders are due and sending them exactly once.
//!
//! The dispatcher takes `now` as an argument rather than reading the clock, so
//! tests can put an event three days out and step through a ladder without
//! sleeping. The background worker is a thin wrapper that supplies `Utc::now()`.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    domain::reminder,
    error::AppError,
    messenger::{Channel, Delivery, Messenger},
};

/// What one pass did. Returned so the worker can log it and tests can assert on
/// it without reading the ledger back.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Dispatch {
    pub sent: u32,
    pub failed: u32,
    /// Rungs that were due but held for quiet hours; they stay due.
    pub held: u32,
}

/// A rung that has come due, with the event context needed to word the message.
struct DueRung {
    schedule_id: Uuid,
    event_id: Uuid,
    title: String,
    timezone: String,
    first_start: DateTime<Utc>,
}

/// A guest who still owes an answer.
struct Unanswered {
    id: Uuid,
    name: String,
    phone: String,
    rsvp_token: Uuid,
}

/// Rungs whose moment has arrived and whose event hasn't started yet.
///
/// A rung stays eligible from its due time until the event begins, rather than
/// expiring on a narrow window. If the process was down for a day, the reminder
/// still goes out — and because the wording is composed from the real remaining
/// time at send, a late one says "in 2 days" rather than repeating the rung's
/// original "in 2 weeks".
async fn due_rungs(pool: &PgPool, now: DateTime<Utc>) -> Result<Vec<DueRung>, AppError> {
    Ok(sqlx::query_as!(
        DueRung,
        r#"
        SELECT rs.id AS schedule_id, e.id AS event_id, e.title, e.timezone,
               parts.first_start AS "first_start!"
        FROM reminder_schedules rs
        JOIN events e ON e.id = rs.event_id
        JOIN LATERAL (
            SELECT min(starts_at) AS first_start FROM sub_events WHERE event_id = e.id
        ) parts ON parts.first_start IS NOT NULL
        WHERE rs.enabled
          AND parts.first_start > $1
          AND $1 >= parts.first_start - make_interval(mins => rs.offset_minutes)
        ORDER BY parts.first_start
        "#,
        now
    )
    .fetch_all(pool)
    .await?)
}

/// Guests of an event with at least one part still unanswered, who we can
/// actually reach. A guest with no phone number is not a failure to record
/// against every rung forever — there is nothing to attempt, so they're simply
/// not in the set. The dashboard surfaces unreachable guests separately.
async fn unanswered_guests(pool: &PgPool, event_id: Uuid) -> Result<Vec<Unanswered>, AppError> {
    Ok(sqlx::query_as!(
        Unanswered,
        r#"
        SELECT DISTINCT g.id, g.name, g.phone AS "phone!", g.rsvp_token
        FROM guests g
        JOIN guest_invites gi ON gi.guest_id = g.id
        WHERE g.event_id = $1
          AND gi.rsvp_status = 'pending'
          AND g.phone IS NOT NULL
        ORDER BY g.name
        "#,
        event_id
    )
    .fetch_all(pool)
    .await?)
}

/// Takes ownership of one (rung, guest) send, returning false if somebody
/// already has it.
///
/// This is the whole double-text defence, and it is deliberately a database
/// constraint rather than a check in application code: the insert either wins
/// or it doesn't, so two workers, a retry, or a restart mid-batch cannot each
/// decide to send. We claim *before* sending — the cost of that ordering is
/// that a crash between claim and send drops one reminder, which is a far
/// better failure than texting a guest twice.
async fn claim(pool: &PgPool, schedule_id: Uuid, guest_id: Uuid, channel: Channel) -> Result<bool, AppError> {
    let claimed = sqlx::query_scalar!(
        r#"
        INSERT INTO reminder_sends (schedule_id, guest_id, channel, status)
        VALUES ($1, $2, $3, 'sent')
        ON CONFLICT (schedule_id, guest_id) DO NOTHING
        RETURNING 1 AS "one!"
        "#,
        schedule_id,
        guest_id,
        channel.as_str()
    )
    .fetch_optional(pool)
    .await?;
    Ok(claimed.is_some())
}

/// Records that a claimed send didn't make it. The row stays — a failed
/// reminder is not retried on the next pass, because a provider failure is
/// usually a bad number rather than a blip, and blind retries against a
/// timeout that actually delivered would double-text.
async fn mark_failed(pool: &PgPool, schedule_id: Uuid, guest_id: Uuid, detail: &str) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE reminder_sends SET status = 'failed', detail = $3
         WHERE schedule_id = $1 AND guest_id = $2",
        schedule_id,
        guest_id,
        detail
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Runs every rung that is due as of `now`.
pub async fn run_due(
    pool: &PgPool,
    messenger: &Messenger,
    public_base_url: &str,
    now: DateTime<Utc>,
) -> Result<Dispatch, AppError> {
    let mut dispatch = Dispatch::default();

    for rung in due_rungs(pool, now).await? {
        // Events carry their own timezone; fall back to Lagos rather than UTC,
        // which would put quiet hours an hour out for every local event.
        let tz: Tz = rung.timezone.parse().unwrap_or(chrono_tz::Africa::Lagos);

        if !reminder::is_sendable_hour(now, tz) {
            // Held, not skipped: the rung stays due and goes out at 08:00.
            dispatch.held += 1;
            tracing::debug!(
                event = %rung.title,
                until = %reminder::next_sendable(now, tz),
                "reminder held for quiet hours"
            );
            continue;
        }

        let when = reminder::time_until(now, rung.first_start, tz);
        for guest in unanswered_guests(pool, rung.event_id).await? {
            // WhatsApp first; the SMS fallback is what a real adapter drops to
            // when WhatsApp has no session with that number.
            let channel = Channel::Whatsapp;
            if !claim(pool, rung.schedule_id, guest.id, channel).await? {
                continue; // already handled by an earlier pass or another worker
            }

            let link = format!("{}/r/{}", public_base_url, guest.rsvp_token);
            let body = reminder::compose(&guest.name, &rung.title, &when, &link);
            match messenger.send(&guest.phone, &body, channel).await {
                Delivery::Sent => dispatch.sent += 1,
                Delivery::Failed(detail) => {
                    tracing::warn!(guest = %guest.name, "reminder failed: {detail}");
                    mark_failed(pool, rung.schedule_id, guest.id, &detail).await?;
                    dispatch.failed += 1;
                }
            }
        }
    }

    Ok(dispatch)
}

/// Polls for due reminders forever. One tick a minute is ample: rungs are days
/// apart, and the ledger makes a missed or repeated tick harmless.
pub fn spawn_worker(pool: PgPool, messenger: Arc<Messenger>, public_base_url: String) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            ticker.tick().await;
            match run_due(&pool, &messenger, &public_base_url, Utc::now()).await {
                Ok(d) if d.sent > 0 || d.failed > 0 => {
                    tracing::info!("reminders: {} sent, {} failed", d.sent, d.failed);
                }
                Ok(_) => {}
                // A bad pass must not kill the worker; the next tick retries.
                Err(err) => tracing::error!("reminder pass failed: {err}"),
            }
        }
    });
}
