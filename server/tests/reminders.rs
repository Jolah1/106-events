mod common;

use axum::http::{Method, StatusCode};
use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Africa::Lagos;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use common::{app, seed_user, send};
use server::{
    messenger::{Messenger, Outbound},
    reminders::{Dispatch, run_due},
};

/// The event starts 20 Nov 2099 at 10:00 Lagos time. Every `at()` in these
/// tests is a Lagos wall-clock reading, which is how an organizer thinks.
fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
    Lagos.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap().with_timezone(&Utc)
}

struct Seeded {
    app: axum::Router,
    session: String,
    event_id: Uuid,
    /// Aunt Ngozi, invited to both parts, hasn't answered.
    pending_guest: Uuid,
}

/// A two-part event with one guest who hasn't answered.
async fn seed(pool: &PgPool) -> Seeded {
    let app = app(pool.clone());
    let (_, session) = seed_user(pool, "organizer@example.com").await;

    let (_, event, _) = send(
        &app,
        Method::POST,
        "/api/events",
        Some(json!({
            "title": "Ada & Tunde",
            "subEvents": [
                { "name": "Engagement", "startsAt": "2099-11-20T09:00:00Z" },
                { "name": "Reception", "startsAt": "2099-11-21T12:00:00Z" }
            ]
        })),
        Some(&session),
    )
    .await;
    let event_id: Uuid = event["id"].as_str().unwrap().parse().unwrap();
    let parts: Vec<Uuid> = event["subEvents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().parse().unwrap())
        .collect();

    let (_, guest, _) = send(
        &app,
        Method::POST,
        &format!("/api/events/{event_id}/guests"),
        Some(json!({
            "name": "Aunt Ngozi",
            "phone": "08066882563",
            "plusOnes": 2,
            "subEventIds": parts
        })),
        Some(&session),
    )
    .await;

    Seeded {
        app,
        session,
        event_id,
        pending_guest: guest["id"].as_str().unwrap().parse().unwrap(),
    }
}

/// Adds a rung `days` before the event through the real API.
async fn add_rung(s: &Seeded, days: i64) -> Uuid {
    let (status, body, _) = send(
        &s.app,
        Method::POST,
        &format!("/api/events/{}/reminders", s.event_id),
        Some(json!({ "offsetMinutes": days * 24 * 60 })),
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["id"].as_str().unwrap().parse().unwrap()
}

/// Runs a pass with a capturing messenger, returning the dispatch and messages.
async fn pass(pool: &PgPool, now: DateTime<Utc>) -> (Dispatch, Vec<Outbound>) {
    let (messenger, captured) = Messenger::capturing();
    let dispatch = run_due(pool, &messenger, "https://106.events", now)
        .await
        .expect("reminder pass");
    let messages = captured.lock().unwrap().clone();
    (dispatch, messages)
}

#[sqlx::test]
async fn a_due_rung_reminds_the_guest_who_hasnt_answered(pool: PgPool) {
    let s = seed(&pool).await;
    add_rung(&s, 3).await;

    // Four days out: the 3-day rung hasn't come due.
    let (dispatch, messages) = pass(&pool, at(2099, 11, 16, 10)).await;
    assert_eq!(dispatch.sent, 0, "a rung that isn't due sends nothing");
    assert!(messages.is_empty());

    // Two days out: due.
    let (dispatch, messages) = pass(&pool, at(2099, 11, 18, 10)).await;
    assert_eq!(dispatch.sent, 1);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].to, "+2348066882563", "sent to the E.164 number");
    assert!(messages[0].body.contains("Ada & Tunde"), "{}", messages[0].body);
    let _ = s;
}

#[sqlx::test]
async fn a_guest_is_never_texted_twice_for_the_same_rung(pool: PgPool) {
    let s = seed(&pool).await;
    add_rung(&s, 3).await;

    // The worker ticks every minute; the rung stays due for days. Only the
    // first pass may send.
    let (first, messages) = pass(&pool, at(2099, 11, 18, 10)).await;
    assert_eq!(first.sent, 1);
    assert_eq!(messages.len(), 1);

    for hour in [11, 12, 18] {
        let (again, messages) = pass(&pool, at(2099, 11, 18, hour)).await;
        assert_eq!(again.sent, 0, "already reminded at this rung");
        assert!(messages.is_empty(), "no second text");
    }

    // And the ledger holds exactly one row.
    let rows = sqlx::query_scalar!(r#"SELECT count(*) AS "c!" FROM reminder_sends"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[sqlx::test]
async fn two_workers_racing_the_same_rung_send_once(pool: PgPool) {
    // Fly.io can run more than one instance, and a deploy overlaps the old
    // process with the new one. The claim is a unique-constrained insert
    // precisely so this case can't double-text; assert it rather than trust it.
    let s = seed(&pool).await;
    add_rung(&s, 3).await;

    let now = at(2099, 11, 18, 10);
    let (a, captured_a) = Messenger::capturing();
    let (b, captured_b) = Messenger::capturing();
    let (ra, rb) = tokio::join!(
        run_due(&pool, &a, "https://106.events", now),
        run_due(&pool, &b, "https://106.events", now),
    );

    let total_sent = ra.unwrap().sent + rb.unwrap().sent;
    let total_messages = captured_a.lock().unwrap().len() + captured_b.lock().unwrap().len();
    assert_eq!(total_sent, 1, "exactly one worker owns the send");
    assert_eq!(total_messages, 1, "the guest's phone buzzes once");
}

#[sqlx::test]
async fn each_rung_is_its_own_nudge(pool: PgPool) {
    let s = seed(&pool).await;
    add_rung(&s, 14).await;
    add_rung(&s, 3).await;

    // Ten days out, only the 14-day rung has fired.
    let (d, _) = pass(&pool, at(2099, 11, 10, 10)).await;
    assert_eq!(d.sent, 1);

    // Two days out the 3-day rung fires too — a separate rung, so the same
    // still-silent guest is nudged again.
    let (d, messages) = pass(&pool, at(2099, 11, 18, 10)).await;
    assert_eq!(d.sent, 1);
    assert!(messages[0].body.contains("in 2 days"), "{}", messages[0].body);
}

#[sqlx::test]
async fn answering_stops_the_reminders(pool: PgPool) {
    let s = seed(&pool).await;
    add_rung(&s, 14).await;
    add_rung(&s, 3).await;

    // The guest confirms via WhatsApp before the first rung.
    let (_, result, _) = send(
        &s.app,
        Method::POST,
        "/api/webhooks/inbound",
        Some(json!({ "channel": "whatsapp", "fromPhone": "+2348066882563", "body": "1" })),
        None,
    )
    .await;
    assert_eq!(result["outcome"], "recorded");

    let (d, messages) = pass(&pool, at(2099, 11, 18, 10)).await;
    assert_eq!(d.sent, 0, "a guest who answered is not a non-responder");
    assert!(messages.is_empty());
}

#[sqlx::test]
async fn a_partial_answer_still_counts_as_owing_one(pool: PgPool) {
    let s = seed(&pool).await;
    add_rung(&s, 3).await;

    // Confirm only the engagement, directly — the reception stays pending.
    sqlx::query!(
        "UPDATE guest_invites SET rsvp_status = 'confirmed', party_size = 1
         WHERE guest_id = $1 AND sub_event_id = (
             SELECT id FROM sub_events WHERE event_id = $2 ORDER BY position LIMIT 1
         )",
        s.pending_guest,
        s.event_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let (d, _) = pass(&pool, at(2099, 11, 18, 10)).await;
    assert_eq!(d.sent, 1, "still owes an answer on the other part");
}

#[sqlx::test]
async fn nobody_is_texted_at_three_in_the_morning(pool: PgPool) {
    let s = seed(&pool).await;
    add_rung(&s, 3).await;

    // The rung comes due overnight.
    let (d, messages) = pass(&pool, at(2099, 11, 18, 3)).await;
    assert_eq!(d.sent, 0);
    assert_eq!(d.held, 1, "held, not skipped");
    assert!(messages.is_empty());

    // Nothing was claimed, so the morning pass still sends it.
    let (d, messages) = pass(&pool, at(2099, 11, 18, 9)).await;
    assert_eq!(d.sent, 1, "the held reminder goes out in the morning");
    assert_eq!(messages.len(), 1);
}

#[sqlx::test]
async fn the_message_carries_the_guests_own_rsvp_link(pool: PgPool) {
    let s = seed(&pool).await;
    add_rung(&s, 3).await;

    let (_, messages) = pass(&pool, at(2099, 11, 18, 10)).await;
    let token = sqlx::query_scalar!("SELECT rsvp_token FROM guests WHERE id = $1", s.pending_guest)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        messages[0].body.contains(&format!("https://106.events/r/{token}")),
        "{}",
        messages[0].body
    );
    assert!(messages[0].body.contains("Aunt Ngozi"), "addressed by name");
}

#[sqlx::test]
async fn a_guest_with_no_phone_is_skipped_not_failed(pool: PgPool) {
    let s = seed(&pool).await;
    add_rung(&s, 3).await;

    // An email-only guest: there is nothing to attempt.
    let (status, _, _) = send(
        &s.app,
        Method::POST,
        &format!("/api/events/{}/guests", s.event_id),
        Some(json!({ "name": "Email Only", "email": "e@example.com" })),
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (d, messages) = pass(&pool, at(2099, 11, 18, 10)).await;
    assert_eq!(d.sent, 1, "only the reachable guest");
    assert_eq!(d.failed, 0, "an unreachable guest is not a delivery failure");
    assert_eq!(messages.len(), 1);
}

#[sqlx::test]
async fn a_failed_send_is_recorded_and_not_retried(pool: PgPool) {
    let s = seed(&pool).await;
    add_rung(&s, 3).await;

    let failing = Messenger::Failing("invalid recipient".into());
    let d = run_due(&pool, &failing, "https://106.events", at(2099, 11, 18, 10))
        .await
        .unwrap();
    assert_eq!(d.failed, 1);
    assert_eq!(d.sent, 0);

    let row = sqlx::query!(r#"SELECT status, detail FROM reminder_sends"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.status, "failed");
    assert_eq!(row.detail.as_deref(), Some("invalid recipient"));

    // A later pass does not try again: a provider rejection is usually a bad
    // number, and blindly retrying a timeout that actually delivered would
    // double-text the guest.
    let (d, messages) = pass(&pool, at(2099, 11, 18, 12)).await;
    assert_eq!(d.sent, 0);
    assert!(messages.is_empty());
}

#[sqlx::test]
async fn reminders_stop_once_the_event_has_started(pool: PgPool) {
    let s = seed(&pool).await;
    add_rung(&s, 3).await;

    // The rung is long past due, but the wedding is underway. A "have you
    // replied?" text now is pure noise.
    let (d, messages) = pass(&pool, at(2099, 11, 20, 11)).await;
    assert_eq!(d.sent, 0);
    assert!(messages.is_empty());
}

#[sqlx::test]
async fn a_late_pass_still_sends_but_tells_the_truth(pool: PgPool) {
    let s = seed(&pool).await;
    add_rung(&s, 14).await;

    // The process was down; the 14-day rung is only reached with 2 days to go.
    // It still goes out, and it says "in 2 days" rather than repeating the
    // rung's original two weeks.
    let (d, messages) = pass(&pool, at(2099, 11, 18, 10)).await;
    assert_eq!(d.sent, 1);
    assert!(messages[0].body.contains("in 2 days"), "{}", messages[0].body);
    assert!(!messages[0].body.contains("2 weeks"));
}

// --- Schedule API -------------------------------------------------------------

#[sqlx::test]
async fn schedules_are_listed_with_what_they_have_done(pool: PgPool) {
    let s = seed(&pool).await;
    add_rung(&s, 14).await;
    add_rung(&s, 3).await;

    let (status, body, _) = send(
        &s.app,
        Method::GET,
        &format!("/api/events/{}/reminders", s.event_id),
        None,
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rungs = body.as_array().unwrap();
    assert_eq!(rungs.len(), 2);
    // Furthest-out rung first: the ladder reads top-down.
    assert_eq!(rungs[0]["offsetMinutes"], 14 * 24 * 60);
    assert_eq!(rungs[0]["sentCount"], 0);

    pass(&pool, at(2099, 11, 10, 10)).await;

    let (_, body, _) = send(
        &s.app,
        Method::GET,
        &format!("/api/events/{}/reminders", s.event_id),
        None,
        Some(&s.session),
    )
    .await;
    assert_eq!(body[0]["sentCount"], 1, "the fired rung reports its reach");
    assert_eq!(body[1]["sentCount"], 0);
}

#[sqlx::test]
async fn the_same_rung_cannot_be_added_twice(pool: PgPool) {
    let s = seed(&pool).await;
    add_rung(&s, 3).await;

    let (status, _, _) = send(
        &s.app,
        Method::POST,
        &format!("/api/events/{}/reminders", s.event_id),
        Some(json!({ "offsetMinutes": 3 * 24 * 60 })),
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "a duplicate rung is a double-text");
}

#[sqlx::test]
async fn schedule_input_is_validated(pool: PgPool) {
    let s = seed(&pool).await;

    for bad in [0, -60, 60 * 24 * 1000] {
        let (status, _, _) = send(
            &s.app,
            Method::POST,
            &format!("/api/events/{}/reminders", s.event_id),
            Some(json!({ "offsetMinutes": bad })),
            Some(&s.session),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "offset {bad}");
    }

    // An unknown event is a 404, not a rung nobody can ever see.
    let (status, _, _) = send(
        &s.app,
        Method::POST,
        &format!("/api/events/{}/reminders", Uuid::new_v4()),
        Some(json!({ "offsetMinutes": 60 })),
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn deleting_a_rung_stops_it(pool: PgPool) {
    let s = seed(&pool).await;
    let rung = add_rung(&s, 3).await;

    let (status, _, _) = send(
        &s.app,
        Method::DELETE,
        &format!("/api/reminders/{rung}"),
        None,
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (d, _) = pass(&pool, at(2099, 11, 18, 10)).await;
    assert_eq!(d.sent, 0, "a deleted rung never fires");
}

#[sqlx::test]
async fn reminders_need_a_session(pool: PgPool) {
    let s = seed(&pool).await;
    let (status, _, _) = send(
        &s.app,
        Method::GET,
        &format!("/api/events/{}/reminders", s.event_id),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
