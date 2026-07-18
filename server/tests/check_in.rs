//! Free attendance at the door.
//!
//! The behaviours worth pinning down are the ones a real door exercises: a
//! guest scanning twice, a phone replaying a queue after the signal comes back,
//! a plus-one who wasn't confirmed for, and a stranger's QR code.

mod common;

use axum::http::{Method, StatusCode};
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use common::{app, seed_user, send};

struct Seeded {
    app: axum::Router,
    session: String,
    event_id: Uuid,
    /// [engagement, reception]
    parts: [Uuid; 2],
    guest_id: Uuid,
    token: Uuid,
}

/// A two-part event with one guest who may bring three others, invited to both
/// parts. Created through the real API so the invites start 'pending' exactly
/// as they do in production.
async fn seed(pool: &PgPool) -> Seeded {
    seed_as(pool, "organizer@example.com").await
}

/// The same, for a second organizer running a different wedding entirely.
async fn seed_as(pool: &PgPool, email: &str) -> Seeded {
    let app = app(pool.clone());
    let (_, session) = seed_user(pool, email).await;

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
    let event_id: Uuid = event["id"].as_str().unwrap().parse().unwrap();
    let parts = [
        event["subEvents"][0]["id"].as_str().unwrap().parse().unwrap(),
        event["subEvents"][1]["id"].as_str().unwrap().parse().unwrap(),
    ];

    let (_, guest, _) = send(
        &app,
        Method::POST,
        &format!("/api/events/{event_id}/guests"),
        Some(json!({
            "name": "Aunt Ngozi",
            "phone": "08066882563",
            "plusOnes": 3,
            "subEventIds": parts
        })),
        Some(&session),
    )
    .await;
    let guest_id: Uuid = guest["id"].as_str().unwrap().parse().unwrap();
    let token = sqlx::query_scalar!("SELECT rsvp_token FROM guests WHERE id = $1", guest_id)
        .fetch_one(pool)
        .await
        .unwrap();

    Seeded {
        app,
        session,
        event_id,
        parts,
        guest_id,
        token,
    }
}

/// Answers the RSVP through the public form, the way a guest does.
async fn rsvp(app: &axum::Router, token: Uuid, form: &str) {
    use tower::ServiceExt;
    let request = axum::http::Request::builder()
        .method(Method::POST)
        .uri(format!("/r/{token}"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(axum::body::Body::from(form.to_string()))
        .unwrap();
    let status = app.clone().oneshot(request).await.unwrap().status();
    assert_eq!(status, StatusCode::SEE_OTHER, "rsvp should redirect");
}

impl Seeded {
    async fn sync(&self) -> Value {
        let (status, body, _) = send(
            &self.app,
            Method::POST,
            &format!("/api/events/{}/attendees/sync", self.event_id),
            None,
            Some(&self.session),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    async fn attendees(&self) -> Vec<Value> {
        let (status, body, _) = send(
            &self.app,
            Method::GET,
            &format!("/api/events/{}/attendees", self.event_id),
            None,
            Some(&self.session),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body.as_array().unwrap().clone()
    }

    /// Scans a code at a part. Always asserts 200: the door must never be told
    /// to retry, whatever the outcome.
    async fn scan(&self, part: Uuid, body: Value) -> Value {
        let (status, result, _) = send(
            &self.app,
            Method::POST,
            &format!("/api/sub-events/{part}/check-in"),
            Some(body),
            Some(&self.session),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "a scan always answers 200: {result}");
        result
    }

    async fn check_ins(&self, part: Uuid) -> Vec<Value> {
        let (status, body, _) = send(
            &self.app,
            Method::GET,
            &format!("/api/sub-events/{part}/check-ins"),
            None,
            Some(&self.session),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body.as_array().unwrap().clone()
    }

    async fn manifest(&self, part: Uuid) -> Value {
        let (status, body, _) = send(
            &self.app,
            Method::GET,
            &format!("/api/sub-events/{part}/door"),
            None,
            Some(&self.session),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }
}

fn codes(attendees: &[Value]) -> Vec<String> {
    attendees
        .iter()
        .map(|a| a["code"].as_str().unwrap().to_string())
        .collect()
}

// --- issuing codes -----------------------------------------------------------

#[sqlx::test]
async fn every_head_gets_its_own_code(pool: PgPool) {
    let s = seed(&pool).await;

    let report = s.sync().await;
    assert_eq!(report["created"], 4, "the guest plus three plus-ones");
    assert_eq!(report["total"], 4);

    let attendees = s.attendees().await;
    let labels: Vec<&str> = attendees
        .iter()
        .map(|a| a["label"].as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        ["Aunt Ngozi", "Aunt Ngozi +1", "Aunt Ngozi +2", "Aunt Ngozi +3"],
        "plus-ones have no names, so the index is the label"
    );

    let issued = codes(&attendees);
    let distinct: std::collections::HashSet<&String> = issued.iter().collect();
    assert_eq!(distinct.len(), 4, "two heads must never share a code");
}

#[sqlx::test]
async fn syncing_again_reissues_nothing(pool: PgPool) {
    let s = seed(&pool).await;
    s.sync().await;
    let before = codes(&s.attendees().await);

    let report = s.sync().await;
    assert_eq!(report["created"], 0, "sync only ever fills gaps");
    assert_eq!(
        codes(&s.attendees().await),
        before,
        "a code already on someone's phone must keep working"
    );
}

#[sqlx::test]
async fn raising_the_plus_ones_adds_codes_and_leaves_the_old_ones_alone(pool: PgPool) {
    let s = seed(&pool).await;
    s.sync().await;
    let before = codes(&s.attendees().await);

    let (status, _, _) = send(
        &s.app,
        Method::PATCH,
        &format!("/api/guests/{}", s.guest_id),
        Some(json!({ "plusOnes": 5 })),
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let report = s.sync().await;
    assert_eq!(report["created"], 2);
    let after = codes(&s.attendees().await);
    assert_eq!(after.len(), 6);
    for code in before {
        assert!(after.contains(&code), "{code} should still be valid");
    }
}

#[sqlx::test]
async fn lowering_the_plus_ones_does_not_revoke_codes_already_sent(pool: PgPool) {
    let s = seed(&pool).await;
    s.sync().await;

    send(
        &s.app,
        Method::PATCH,
        &format!("/api/guests/{}", s.guest_id),
        Some(json!({ "plusOnes": 0 })),
        Some(&s.session),
    )
    .await;
    s.sync().await;

    assert_eq!(
        s.attendees().await.len(),
        4,
        "the rows stay; the allowance check at the door is what enforces the \
         smaller number, not a code that stops scanning"
    );
}

// --- the door ----------------------------------------------------------------

#[sqlx::test]
async fn a_confirmed_guest_is_admitted(pool: PgPool) {
    let s = seed(&pool).await;
    let [_, reception] = s.parts;
    rsvp(&s.app, s.token, &format!("attending={reception}&party_{reception}=2")).await;
    s.sync().await;

    let code = codes(&s.attendees().await)[0].clone();
    let result = s.scan(reception, json!({ "code": code })).await;

    assert_eq!(result["outcome"], "admitted");
    assert_eq!(result["label"], "Aunt Ngozi");
    assert_eq!(result["partyCheckedIn"], 1);
    assert_eq!(result["partyAllowed"], 2, "the door can say 1 of 2");
    assert!(result["checkedInAt"].is_string());
}

#[sqlx::test]
async fn staff_can_type_the_code_off_the_screen(pool: PgPool) {
    let s = seed(&pool).await;
    let [_, reception] = s.parts;
    rsvp(&s.app, s.token, &format!("attending={reception}&party_{reception}=1")).await;
    s.sync().await;

    // What someone reads aloud and someone else types: lowercase, grouped.
    let code = codes(&s.attendees().await)[0].clone();
    let typed = format!("{}-{}", &code[..4].to_lowercase(), &code[4..].to_lowercase());

    let result = s.scan(reception, json!({ "code": typed })).await;
    assert_eq!(result["outcome"], "admitted", "{result}");
}

#[sqlx::test]
async fn scanning_the_same_code_twice_admits_one_person(pool: PgPool) {
    let s = seed(&pool).await;
    let [_, reception] = s.parts;
    rsvp(&s.app, s.token, &format!("attending={reception}&party_{reception}=4")).await;
    s.sync().await;

    let code = codes(&s.attendees().await)[0].clone();
    let first = s.scan(reception, json!({ "code": code })).await;
    let second = s.scan(reception, json!({ "code": code })).await;

    assert_eq!(first["outcome"], "admitted");
    assert_eq!(second["outcome"], "already_in", "a double-tap is not a second head");
    assert_eq!(second["partyCheckedIn"], 1);
    assert_eq!(s.check_ins(reception).await.len(), 1);
}

#[sqlx::test]
async fn a_replayed_offline_queue_converges_on_one_check_in(pool: PgPool) {
    let s = seed(&pool).await;
    let [_, reception] = s.parts;
    rsvp(&s.app, s.token, &format!("attending={reception}&party_{reception}=4")).await;
    s.sync().await;
    let issued = codes(&s.attendees().await);

    // A phone that lost signal queues four scans, then sends the whole queue
    // three times because the first two attempts timed out.
    let scanned_at = "2099-11-21T14:05:00Z";
    for _ in 0..3 {
        for code in &issued {
            s.scan(
                reception,
                json!({ "code": code, "offline": true, "scannedAt": scanned_at }),
            )
            .await;
        }
    }

    let records = s.check_ins(reception).await;
    assert_eq!(records.len(), 4, "four heads, however many times the queue replayed");
    for record in &records {
        assert_eq!(record["syncedOffline"], true);
        assert_eq!(
            record["checkedInAt"].as_str().unwrap(),
            "2099-11-21T14:05:00Z",
            "the count reflects when they walked in, not when the signal came back"
        );
    }
}

#[sqlx::test]
async fn a_head_beyond_the_confirmed_party_needs_a_decision(pool: PgPool) {
    let s = seed(&pool).await;
    let [_, reception] = s.parts;
    // Confirmed for two, but all four turn up.
    rsvp(&s.app, s.token, &format!("attending={reception}&party_{reception}=2")).await;
    s.sync().await;
    let issued = codes(&s.attendees().await);

    for code in &issued[..2] {
        assert_eq!(s.scan(reception, json!({ "code": code })).await["outcome"], "admitted");
    }

    let third = s.scan(reception, json!({ "code": &issued[2] })).await;
    assert_eq!(third["outcome"], "over_allowance", "not a refusal — a question");
    assert_eq!(third["partyCheckedIn"], 2);
    assert_eq!(third["partyAllowed"], 2);
    assert_eq!(s.check_ins(reception).await.len(), 2, "nothing recorded yet");

    // Staff wave them in anyway.
    let overridden = s
        .scan(reception, json!({ "code": &issued[2], "allowOver": true }))
        .await;
    assert_eq!(overridden["outcome"], "admitted");

    let records = s.check_ins(reception).await;
    assert_eq!(records.len(), 3);
    let over: Vec<&Value> = records
        .iter()
        .filter(|r| r["overAllowance"] == true)
        .collect();
    assert_eq!(over.len(), 1, "the overage is recorded, not silently absorbed");
    assert_eq!(over[0]["label"], "Aunt Ngozi +2");
}

#[sqlx::test]
async fn someone_who_declined_is_not_waved_through_silently(pool: PgPool) {
    let s = seed(&pool).await;
    let [_, reception] = s.parts;
    rsvp(&s.app, s.token, "").await; // nothing ticked: declined everything
    s.sync().await;

    let code = codes(&s.attendees().await)[0].clone();
    let result = s.scan(reception, json!({ "code": code })).await;
    assert_eq!(result["outcome"], "over_allowance");
    assert_eq!(result["partyAllowed"], 0);

    // But they are standing right there, so staff can still admit them.
    let result = s
        .scan(reception, json!({ "code": code, "allowOver": true }))
        .await;
    assert_eq!(result["outcome"], "admitted");
    assert_eq!(s.check_ins(reception).await[0]["overAllowance"], true);
}

#[sqlx::test]
async fn a_code_for_another_part_is_named_precisely(pool: PgPool) {
    let s = seed(&pool).await;
    let [engagement, reception] = s.parts;
    rsvp(&s.app, s.token, &format!("attending={reception}&party_{reception}=1")).await;
    s.sync().await;

    // Uninvite the guest from the engagement, then scan their code there.
    sqlx::query!(
        "DELETE FROM guest_invites WHERE guest_id = $1 AND sub_event_id = $2",
        s.guest_id,
        engagement
    )
    .execute(&pool)
    .await
    .unwrap();

    let code = codes(&s.attendees().await)[0].clone();
    let result = s.scan(engagement, json!({ "code": code })).await;
    assert_eq!(result["outcome"], "not_invited");
    assert_eq!(
        result["guestName"], "Aunt Ngozi",
        "staff need the name to sort it out, not just a refusal"
    );
    assert!(s.check_ins(engagement).await.is_empty());
}

#[sqlx::test]
async fn a_code_from_another_event_does_not_open_this_door(pool: PgPool) {
    let s = seed(&pool).await;
    let [_, reception] = s.parts;
    rsvp(&s.app, s.token, &format!("attending={reception}&party_{reception}=1")).await;
    s.sync().await;

    // A guest of somebody else's wedding, with a perfectly valid code.
    let other = seed_as(&pool, "other-organizer@example.com").await;
    other.sync().await;
    let foreign = codes(&other.attendees().await)[0].clone();

    let result = s.scan(reception, json!({ "code": foreign })).await;
    assert_eq!(result["outcome"], "not_invited");
    assert!(s.check_ins(reception).await.is_empty());
}

#[sqlx::test]
async fn a_stranger_qr_is_rejected_before_it_reaches_the_database(pool: PgPool) {
    let s = seed(&pool).await;
    let [_, reception] = s.parts;
    s.sync().await;

    for junk in ["WIFI:S:MyNetwork;", "", "https://example.com/promo", "ACHD4F7"] {
        let result = s.scan(reception, json!({ "code": junk })).await;
        assert_eq!(result["outcome"], "unknown_code", "scanned {junk:?}");
    }
}

// --- extras at the door ------------------------------------------------------

#[sqlx::test]
async fn an_extra_head_added_at_the_door_survives_a_later_sync(pool: PgPool) {
    let s = seed(&pool).await;
    let [_, reception] = s.parts;
    rsvp(&s.app, s.token, &format!("attending={reception}&party_{reception}=4")).await;
    s.sync().await;

    // Someone nobody counted arrives with the family.
    let (status, extra, _) = send(
        &s.app,
        Method::POST,
        &format!("/api/guests/{}/extra-head", s.guest_id),
        None,
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{extra}");
    let extra_code = extra["code"].as_str().unwrap().to_string();

    // The organizer later raises the allowance, which used to reuse the extra's
    // head index and blow up on the unique constraint.
    send(
        &s.app,
        Method::PATCH,
        &format!("/api/guests/{}", s.guest_id),
        Some(json!({ "plusOnes": 5 })),
        Some(&s.session),
    )
    .await;
    let report = s.sync().await;
    assert_eq!(report["created"], 2, "two more invited heads");
    assert_eq!(report["total"], 7, "four invited, one extra, two new");

    let attendees = s.attendees().await;
    assert!(
        codes(&attendees).contains(&extra_code),
        "the door's code must keep working"
    );
    let extras: Vec<&Value> = attendees.iter().filter(|a| a["isExtra"] == true).collect();
    assert_eq!(extras.len(), 1, "the organizer can see one uncounted arrival");

    let result = s
        .scan(reception, json!({ "code": extra_code, "allowOver": true }))
        .await;
    assert_eq!(result["outcome"], "admitted", "{result}");
}

// --- working with no signal --------------------------------------------------

#[sqlx::test]
async fn the_door_manifest_lets_a_disconnected_scanner_do_its_job(pool: PgPool) {
    let s = seed(&pool).await;
    let [_, reception] = s.parts;
    rsvp(&s.app, s.token, &format!("attending={reception}&party_{reception}=2")).await;
    s.sync().await;
    let issued = codes(&s.attendees().await);

    // One head is already through when the device syncs.
    s.scan(reception, json!({ "code": &issued[0] })).await;

    let manifest = s.manifest(reception).await;
    assert_eq!(manifest["subEventName"], "Reception");
    assert_eq!(manifest["eventTitle"], "Ada & Tunde");
    let generated: DateTime<Utc> = manifest["generatedAt"].as_str().unwrap().parse().unwrap();
    assert!((Utc::now() - generated).abs() < Duration::minutes(1));

    let entries = manifest["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 4, "every head, so a stranger's code fails locally");
    for entry in entries {
        assert_eq!(entry["partyAllowed"], 2);
    }
    assert_eq!(entries[0]["checkedIn"], true, "already through before doors synced");
    assert_eq!(entries[1]["checkedIn"], false);
    assert!(entries.iter().any(|e| e["code"] == issued[0].as_str()));
}

#[sqlx::test]
async fn syncing_an_event_that_does_not_exist_is_a_404(pool: PgPool) {
    let s = seed(&pool).await;
    let (status, _, _) = send(
        &s.app,
        Method::POST,
        &format!("/api/events/{}/attendees/sync", Uuid::new_v4()),
        None,
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn the_door_is_staff_only(pool: PgPool) {
    let s = seed(&pool).await;
    let [_, reception] = s.parts;
    s.sync().await;
    let code = codes(&s.attendees().await)[0].clone();

    let (status, _, _) = send(
        &s.app,
        Method::POST,
        &format!("/api/sub-events/{reception}/check-in"),
        Some(json!({ "code": code })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _, _) = send(
        &s.app,
        Method::GET,
        &format!("/api/sub-events/{reception}/door"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
