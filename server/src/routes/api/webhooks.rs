//! Inbound WhatsApp and SMS replies.
//!
//! Providers differ (the WhatsApp Business API, Termii, Africa's Talking), and
//! each posts its own payload shape and signs it its own way. Rather than bake
//! one provider in, this exposes a single *normalized* endpoint: a thin
//! per-provider adapter translates that provider's webhook into the shape here
//! and presents the shared secret. Everything downstream — matching a sender to
//! a guest, interpreting the reply, recording the RSVP — is provider-agnostic
//! and fully tested.
//!
//! What still needs a provider account to finish: the adapter that maps each
//! provider's JSON and verifies its signature. That is deliberately the only
//! provider-specific piece.

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    domain::{phone, rsvp},
    error::AppError,
    routes::api::rsvp_store::{self, BulkResponse, Channel},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/webhooks/inbound", post(inbound))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundBody {
    /// "whatsapp" or "sms".
    pub channel: String,
    /// The sender, in whatever form the provider gives; normalized here.
    pub from_phone: String,
    /// The message text.
    pub body: String,
    /// The provider's message id, if any. Deduplicates webhook retries.
    #[serde(default)]
    pub provider_ref: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundResult {
    /// How the message was handled, for the adapter's logs and our tests:
    /// recorded, unclear, unknown_sender, or duplicate.
    pub outcome: &'static str,
}

fn channel_from_str(s: &str) -> Result<Channel, AppError> {
    match s {
        "whatsapp" => Ok(Channel::Whatsapp),
        "sms" => Ok(Channel::Sms),
        _ => Err(AppError::validation("channel must be 'whatsapp' or 'sms'")),
    }
}

/// Verifies the shared secret. Absent config means an open webhook, which is
/// only acceptable in development, so it's loud about it.
fn check_secret(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    match &state.config.webhook_secret {
        None => {
            tracing::warn!(
                "WEBHOOK_SECRET not set: the inbound webhook is unauthenticated (development only)"
            );
            Ok(())
        }
        Some(secret) => {
            let presented = headers
                .get("x-webhook-secret")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default();
            // Length-first compare is fine here; the secret isn't a password
            // hash and the endpoint is rate-limited by the provider.
            if presented == secret {
                Ok(())
            } else {
                Err(AppError::Unauthorized)
            }
        }
    }
}

async fn inbound(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<InboundBody>,
) -> Result<(StatusCode, Json<InboundResult>), AppError> {
    check_secret(&state, &headers)?;
    let channel = channel_from_str(&body.channel)?;

    // A retried webhook must not double-record. If we've already logged this
    // provider_ref, acknowledge and stop.
    if let Some(reference) = body.provider_ref.as_deref()
        && already_seen(&state, &body.channel, reference).await?
    {
        return Ok(ok("duplicate"));
    }

    // A sender we can't even parse as a phone number can't be matched; log it
    // as an unknown sender rather than erroring, so the provider doesn't retry.
    let normalized = phone::normalize(&body.from_phone);
    let guest = match &normalized {
        Some(number) => rsvp_store::guest_by_phone(&state.pool, number).await?,
        None => None,
    };

    let reply = rsvp::interpret(&body.body);
    let parsed_as = match (&guest, reply) {
        (None, _) => "unknown_sender",
        (Some(_), rsvp::Reply::Confirm) => "confirmed",
        (Some(_), rsvp::Reply::Decline) => "declined",
        (Some(_), rsvp::Reply::Unclear) => "unclear",
    };

    // Record the raw inbound first, so even a message we can't act on is
    // visible to the organizer.
    log_inbound(&state, &body, guest.as_ref().map(|g| g.id), parsed_as).await?;

    // Only a clearly-understood reply from a known guest changes RSVP state.
    let outcome = match (guest, reply) {
        (Some(guest), rsvp::Reply::Confirm) => {
            rsvp_store::record_bulk_response(&state.pool, &guest, BulkResponse::Confirm, channel)
                .await?;
            "recorded"
        }
        (Some(guest), rsvp::Reply::Decline) => {
            rsvp_store::record_bulk_response(&state.pool, &guest, BulkResponse::Decline, channel)
                .await?;
            "recorded"
        }
        (Some(_), rsvp::Reply::Unclear) => "unclear",
        (None, _) => "unknown_sender",
    };

    Ok(ok(outcome))
}

fn ok(outcome: &'static str) -> (StatusCode, Json<InboundResult>) {
    (StatusCode::OK, Json(InboundResult { outcome }))
}

async fn already_seen(state: &AppState, channel: &str, reference: &str) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar!(
        "SELECT 1 FROM inbound_messages WHERE channel = $1 AND provider_ref = $2",
        channel,
        reference
    )
    .fetch_optional(&state.pool)
    .await?
    .is_some())
}

async fn log_inbound(
    state: &AppState,
    body: &InboundBody,
    guest_id: Option<Uuid>,
    parsed_as: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO inbound_messages (channel, provider_ref, from_phone, body, guest_id, parsed_as)
         VALUES ($1, $2, $3, $4, $5, $6)",
        body.channel,
        body.provider_ref,
        body.from_phone,
        body.body,
        guest_id,
        parsed_as
    )
    .execute(&state.pool)
    .await?;
    Ok(())
}
