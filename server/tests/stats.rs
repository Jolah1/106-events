//! The organizer's rollup.
//!
//! These pin the arithmetic: a rollup that miscounts is worse than none,
//! because catering cooks and chairs get hired off these numbers. Everything
//! is seeded through the real API so the counts summarize rows exactly as
//! production writes them.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use common::{app, seed_user, send};

const fn naira(amount: i64) -> i64 {
    amount * 100
}

struct Seeded {
    app: axum::Router,
    session: String,
    event_id: Uuid,
    /// [engagement, reception]
    parts: [Uuid; 2],
}

/// A two-part event with no guests yet.
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
                { "name": "Engagement", "startsAt": "2099-11-20T10:00:00Z" },
                { "name": "Reception", "startsAt": "2099-11-21T13:00:00Z" }
            ]
        })),
        Some(&session),
    )
    .await;
    Seeded {
        event_id: event["id"].as_str().unwrap().parse().unwrap(),
        parts: [
            event["subEvents"][0]["id"].as_str().unwrap().parse().unwrap(),
            event["subEvents"][1]["id"].as_str().unwrap().parse().unwrap(),
        ],
        app,
        session,
    }
}

impl Seeded {
    async fn stats(&self) -> Value {
        let (status, body, _) = send(
            &self.app,
            Method::GET,
            &format!("/api/events/{}/stats", self.event_id),
            None,
            Some(&self.session),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    /// Adds a guest with an allowance, invited to the given parts; returns
    /// (guest_id, rsvp_token).
    async fn guest(&self, name: &str, plus_ones: i64, parts: &[Uuid]) -> (Uuid, Uuid) {
        let (status, guest, _) = send(
            &self.app,
            Method::POST,
            &format!("/api/events/{}/guests", self.event_id),
            Some(json!({ "name": name, "plusOnes": plus_ones, "subEventIds": parts })),
            Some(&self.session),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{guest}");
        (
            guest["id"].as_str().unwrap().parse().unwrap(),
            guest["rsvpToken"].as_str().unwrap().parse().unwrap(),
        )
    }

    /// Answers the RSVP through the public form, the way a guest does.
    async fn rsvp(&self, token: Uuid, form: &str) {
        use tower::ServiceExt;
        let request = axum::http::Request::builder()
            .method(Method::POST)
            .uri(format!("/r/{token}"))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(axum::body::Body::from(form.to_string()))
            .unwrap();
        let status = self.app.clone().oneshot(request).await.unwrap().status();
        assert_eq!(status, StatusCode::SEE_OTHER, "rsvp should redirect");
    }

    /// Issues passes, then returns each head's door code.
    async fn codes(&self) -> Vec<String> {
        send(
            &self.app,
            Method::POST,
            &format!("/api/events/{}/attendees/sync", self.event_id),
            None,
            Some(&self.session),
        )
        .await;
        let (_, attendees, _) = send(
            &self.app,
            Method::GET,
            &format!("/api/events/{}/attendees", self.event_id),
            None,
            Some(&self.session),
        )
        .await;
        attendees
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["code"].as_str().unwrap().to_string())
            .collect()
    }

    async fn scan(&self, part: Uuid, body: Value) -> Value {
        let (status, result, _) = send(
            &self.app,
            Method::POST,
            &format!("/api/sub-events/{part}/check-in"),
            Some(body),
            Some(&self.session),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{result}");
        result
    }
}

#[sqlx::test]
async fn an_untouched_event_rolls_up_to_zeros_not_errors(pool: PgPool) {
    let s = seed(&pool).await;
    let stats = s.stats().await;

    assert_eq!(stats["guestCount"], 0);
    assert_eq!(stats["headsInvited"], 0);
    assert_eq!(stats["repliedGuests"], 0);
    assert_eq!(stats["awaitingGuests"], 0);
    assert_eq!(stats["vendorCount"], 0);
    assert_eq!(stats["vendorOutstandingKobo"], 0);

    // Both parts appear even with nobody invited: a part missing from the
    // rollup reads as "deleted", not "quiet".
    let parts = stats["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0]["name"], "Engagement", "ordered by position");
    assert_eq!(parts[0]["invitedParties"], 0);
    assert_eq!(parts[0]["confirmedHeads"], 0);
    assert_eq!(parts[0]["checkedInHeads"], 0);
}

#[sqlx::test]
async fn rsvp_standings_are_counted_per_part(pool: PgPool) {
    let s = seed(&pool).await;
    let [engagement, reception] = s.parts;

    // Ngozi (+3, both parts): confirms the reception with 4, declines the
    // engagement. Bola (+1, reception only): confirms alone. Chidi (+0, both):
    // never answers.
    let (_, ngozi) = s.guest("Aunt Ngozi", 3, &s.parts).await;
    let (_, bola) = s.guest("Bola", 1, &[reception]).await;
    s.guest("Chidi", 0, &s.parts).await;
    s.rsvp(ngozi, &format!("attending={reception}&party_{reception}=4"))
        .await;
    s.rsvp(bola, &format!("attending={reception}&party_{reception}=1"))
        .await;

    let stats = s.stats().await;
    assert_eq!(stats["guestCount"], 3);
    assert_eq!(stats["headsInvited"], 4 + 2 + 1);
    assert_eq!(stats["repliedGuests"], 2, "Ngozi and Bola answered everything");
    assert_eq!(stats["awaitingGuests"], 1, "Chidi still needs chasing");

    let parts = stats["parts"].as_array().unwrap();
    let eng = &parts[0];
    assert_eq!(eng["subEventId"], engagement.to_string());
    assert_eq!(eng["invitedParties"], 2, "Ngozi and Chidi");
    assert_eq!(eng["confirmedParties"], 0);
    assert_eq!(eng["declinedParties"], 1, "unticked means can't make it");
    assert_eq!(eng["pendingParties"], 1);
    assert_eq!(eng["confirmedHeads"], 0);

    let rec = &parts[1];
    assert_eq!(rec["invitedParties"], 3);
    assert_eq!(rec["confirmedParties"], 2);
    assert_eq!(rec["pendingParties"], 1);
    assert_eq!(rec["confirmedHeads"], 5, "Ngozi's 4 plus Bola alone");
}

#[sqlx::test]
async fn a_guest_is_awaiting_until_every_part_has_an_answer(pool: PgPool) {
    let s = seed(&pool).await;
    let [engagement, reception] = s.parts;
    let (_, token) = s.guest("Aunt Ngozi", 0, &s.parts).await;

    // The public form answers every listed part at once (unticked = declined),
    // so flip one part back to pending directly to model a partial answer,
    // e.g. one that arrived over WhatsApp for a single part.
    s.rsvp(token, &format!("attending={reception}&party_{reception}=1"))
        .await;
    sqlx::query!(
        "UPDATE guest_invites SET rsvp_status = 'pending', party_size = 0,
                responded_at = NULL, responded_via = NULL
         WHERE sub_event_id = $1",
        engagement
    )
    .execute(&pool)
    .await
    .unwrap();

    let stats = s.stats().await;
    assert_eq!(stats["repliedGuests"], 0, "half an answer isn't replied");
    assert_eq!(stats["awaitingGuests"], 1);
}

#[sqlx::test]
async fn the_door_count_includes_overrides_and_offline_syncs(pool: PgPool) {
    let s = seed(&pool).await;
    let [_, reception] = s.parts;
    let (_, token) = s.guest("Aunt Ngozi", 3, &s.parts).await;

    // Confirmed for 2 of an allowance of 4. Every head of the allowance gets
    // a code — the door decides over-allowance at scan time, not issue time.
    s.rsvp(token, &format!("attending={reception}&party_{reception}=2"))
        .await;
    let codes = s.codes().await;
    assert_eq!(codes.len(), 4);

    // One live scan, one that synced after the signal came back.
    s.scan(reception, json!({ "code": codes[0] })).await;
    let synced = s
        .scan(reception, json!({ "code": codes[1], "offline": true }))
        .await;
    assert_eq!(synced["outcome"], "admitted");

    // A third head turns up anyway. The door warns, staff wave them through,
    // and the override is recorded — not silently absorbed.
    let warned = s.scan(reception, json!({ "code": codes[2] })).await;
    assert_eq!(warned["outcome"], "over_allowance", "{warned}");
    let over = s
        .scan(reception, json!({ "code": codes[2], "allowOver": true }))
        .await;
    assert_eq!(over["outcome"], "admitted", "{over}");

    let stats = s.stats().await;
    let rec = &stats["parts"][1];
    assert_eq!(rec["confirmedHeads"], 2);
    assert_eq!(rec["checkedInHeads"], 3, "two passes plus the walk-in");
    assert_eq!(rec["overAllowanceHeads"], 1);
    assert_eq!(rec["offlineSyncedHeads"], 1);

    // The engagement's door never opened.
    assert_eq!(stats["parts"][0]["checkedInHeads"], 0);
}

#[sqlx::test]
async fn vendor_money_is_totalled_with_debt_clamped_per_vendor(pool: PgPool) {
    let s = seed(&pool).await;
    for (name, cost, paid) in [
        ("Ronke's Kitchen", naira(150_000), naira(50_000)),
        // Overpaid: must not offset the caterer's ₦100k debt in the total.
        ("DJ Spinall", naira(200_000), naira(250_000)),
    ] {
        let (status, body, _) = send(
            &s.app,
            Method::POST,
            &format!("/api/events/{}/vendors", s.event_id),
            Some(json!({ "name": name, "costKobo": cost, "amountPaidKobo": paid })),
            Some(&s.session),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    let stats = s.stats().await;
    assert_eq!(stats["vendorCount"], 2);
    assert_eq!(stats["vendorCostKobo"], naira(350_000));
    assert_eq!(stats["vendorPaidKobo"], naira(300_000));
    assert_eq!(stats["vendorOutstandingKobo"], naira(100_000));
}

#[sqlx::test]
async fn the_rollup_is_staff_only_and_scoped_by_existence(pool: PgPool) {
    let s = seed(&pool).await;

    let (status, _, _) = send(
        &s.app,
        Method::GET,
        &format!("/api/events/{}/stats", s.event_id),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _, _) = send(
        &s.app,
        Method::GET,
        &format!("/api/events/{}/stats", Uuid::new_v4()),
        None,
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
