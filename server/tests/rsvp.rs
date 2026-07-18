mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use common::{app, seed_user, send};

/// Seeds a two-part event and one guest invited to both, returning the guest's
/// id, RSVP token, and the two part ids. Goes through the real API so the guest
/// (and its invites, with their default 'pending' RSVP state) are created the
/// way the app creates them.
async fn seed(pool: &PgPool) -> (axum::Router, Uuid, Uuid, [Uuid; 2]) {
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
    let parts = [
        event["subEvents"][0]["id"].as_str().unwrap().parse().unwrap(),
        event["subEvents"][1]["id"].as_str().unwrap().parse().unwrap(),
    ];
    let event_id: Uuid = event["id"].as_str().unwrap().parse().unwrap();

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

    // The RSVP token isn't returned by the guests API (it's a guest-facing
    // secret), so read it straight from the row for the link tests.
    let token = sqlx::query_scalar!("SELECT rsvp_token FROM guests WHERE id = $1", guest_id)
        .fetch_one(pool)
        .await
        .unwrap();

    (app, guest_id, token, parts)
}

/// The RSVP state of each part, keyed by part id, read straight from the DB.
async fn states(pool: &PgPool, guest_id: Uuid) -> Vec<(Uuid, String, i32)> {
    sqlx::query!(
        "SELECT sub_event_id, rsvp_status, party_size FROM guest_invites
         WHERE guest_id = $1 ORDER BY sub_event_id",
        guest_id
    )
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .map(|r| (r.sub_event_id, r.rsvp_status, r.party_size))
    .collect()
}

fn status_of(states: &[(Uuid, String, i32)], part: Uuid) -> (&str, i32) {
    states
        .iter()
        .find(|(id, _, _)| *id == part)
        .map(|(_, s, n)| (s.as_str(), *n))
        .expect("part has an invite row")
}

async fn get_rsvp(app: &axum::Router, token: Uuid) -> (StatusCode, String) {
    let request = axum::http::Request::builder()
        .uri(format!("/r/{token}"))
        .body(axum::body::Body::empty())
        .unwrap();
    use tower::ServiceExt;
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// Posts a form-encoded RSVP to the public link.
async fn post_rsvp(app: &axum::Router, token: Uuid, form: &str) -> StatusCode {
    let request = axum::http::Request::builder()
        .method(Method::POST)
        .uri(format!("/r/{token}"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(axum::body::Body::from(form.to_string()))
        .unwrap();
    use tower::ServiceExt;
    app.clone().oneshot(request).await.unwrap().status()
}

#[sqlx::test]
async fn invites_start_pending(pool: PgPool) {
    let (_, guest_id, _, _) = seed(&pool).await;
    for (_, status, party) in states(&pool, guest_id).await {
        assert_eq!(status, "pending");
        assert_eq!(party, 0);
    }
}

#[sqlx::test]
async fn the_public_page_shows_the_guest_their_parts(pool: PgPool) {
    let (app, _, token, _) = seed(&pool).await;
    let (status, html) = get_rsvp(&app, token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Aunt Ngozi"), "greets the guest by name: {html}");
    assert!(html.contains("Engagement"));
    assert!(html.contains("Reception"));
    assert!(!html.contains("<script"), "public pages ship no JS");
}

#[sqlx::test]
async fn an_unknown_token_is_a_branded_404(pool: PgPool) {
    let (app, _, _, _) = seed(&pool).await;
    let (status, html) = get_rsvp(&app, Uuid::new_v4()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(html.contains("106"), "still branded: {html}");
}

#[sqlx::test]
async fn a_guest_can_confirm_one_part_and_decline_another(pool: PgPool) {
    let (app, guest_id, token, [engagement, reception]) = seed(&pool).await;

    // Attend the reception with two people; skip the engagement.
    let form = format!(
        "attending={reception}&party_{reception}=2&party_{engagement}=1"
    );
    assert_eq!(post_rsvp(&app, token, &form).await, StatusCode::SEE_OTHER);

    let s = states(&pool, guest_id).await;
    assert_eq!(status_of(&s, reception), ("confirmed", 2));
    assert_eq!(status_of(&s, engagement), ("declined", 0), "unticked part is a decline");
}

#[sqlx::test]
async fn party_size_is_capped_at_the_guests_allowance(pool: PgPool) {
    // The guest may bring 3 (plusOnes), so the ceiling is 4 including them.
    let (app, guest_id, token, [engagement, reception]) = seed(&pool).await;

    let form = format!(
        "attending={engagement}&attending={reception}&party_{engagement}=9&party_{reception}=4"
    );
    assert_eq!(post_rsvp(&app, token, &form).await, StatusCode::SEE_OTHER);

    let s = states(&pool, guest_id).await;
    assert_eq!(status_of(&s, engagement), ("confirmed", 4), "9 is clamped to the allowance of 4");
    assert_eq!(status_of(&s, reception), ("confirmed", 4));
}

#[sqlx::test]
async fn a_guest_can_change_their_mind(pool: PgPool) {
    let (app, guest_id, token, [engagement, reception]) = seed(&pool).await;

    // First: confirm both.
    let form = format!(
        "attending={engagement}&attending={reception}&party_{engagement}=1&party_{reception}=1"
    );
    assert_eq!(post_rsvp(&app, token, &form).await, StatusCode::SEE_OTHER);
    let s = states(&pool, guest_id).await;
    assert_eq!(status_of(&s, engagement).0, "confirmed");

    // Then: decline everything (submit with nothing ticked).
    assert_eq!(post_rsvp(&app, token, "").await, StatusCode::SEE_OTHER);
    let s = states(&pool, guest_id).await;
    assert_eq!(status_of(&s, engagement), ("declined", 0), "a later response overrides");
    assert_eq!(status_of(&s, reception), ("declined", 0));
}

// --- WhatsApp / SMS inbound ---------------------------------------------------

async fn inbound(app: &axum::Router, body: Value) -> (StatusCode, Value) {
    let (status, value, _) = send(app, Method::POST, "/api/webhooks/inbound", Some(body), None).await;
    (status, value)
}

#[sqlx::test]
async fn a_whatsapp_yes_confirms_every_invited_part(pool: PgPool) {
    let (app, guest_id, _, _) = seed(&pool).await;

    let (status, result) = inbound(
        &app,
        json!({ "channel": "whatsapp", "fromPhone": "+2348066882563", "body": "1" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["outcome"], "recorded");

    // A coarse confirm assumes the full allowance (1 + 3 plus-ones = 4).
    for (_, s, party) in states(&pool, guest_id).await {
        assert_eq!(s, "confirmed");
        assert_eq!(party, 4);
    }
}

#[sqlx::test]
async fn an_sms_no_declines_every_part(pool: PgPool) {
    let (app, guest_id, _, _) = seed(&pool).await;

    let (_, result) = inbound(
        &app,
        json!({ "channel": "sms", "fromPhone": "08066882563", "body": "Sorry, can't make it" }),
    )
    .await;
    assert_eq!(result["outcome"], "recorded");
    for (_, s, party) in states(&pool, guest_id).await {
        assert_eq!(s, "declined");
        assert_eq!(party, 0);
    }
}

#[sqlx::test]
async fn an_unclear_reply_changes_nothing_but_is_logged(pool: PgPool) {
    let (app, guest_id, _, _) = seed(&pool).await;

    let (_, result) = inbound(
        &app,
        json!({ "channel": "whatsapp", "fromPhone": "+2348066882563", "body": "what's the dress code?" }),
    )
    .await;
    assert_eq!(result["outcome"], "unclear");
    for (_, s, _) in states(&pool, guest_id).await {
        assert_eq!(s, "pending", "an unclear reply leaves the RSVP untouched");
    }
    // But it was recorded for the organizer to see.
    let logged = sqlx::query_scalar!(
        r#"SELECT parsed_as FROM inbound_messages WHERE guest_id = $1"#,
        guest_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(logged, "unclear");
}

#[sqlx::test]
async fn a_reply_from_an_unknown_number_is_kept_not_dropped(pool: PgPool) {
    let (app, _, _, _) = seed(&pool).await;

    let (status, result) = inbound(
        &app,
        json!({ "channel": "sms", "fromPhone": "08000000000", "body": "yes" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "providers must get a 2xx or they retry");
    assert_eq!(result["outcome"], "unknown_sender");

    let logged = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM inbound_messages WHERE parsed_as = 'unknown_sender'"#
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(logged, 1, "an unrecognised sender is surfaced, not lost");
}

#[sqlx::test]
async fn a_retried_webhook_does_not_double_record(pool: PgPool) {
    let (app, guest_id, _, _) = seed(&pool).await;

    let msg = json!({
        "channel": "whatsapp",
        "fromPhone": "+2348066882563",
        "body": "1",
        "providerRef": "wamid.ABCD1234"
    });
    let (_, first) = inbound(&app, msg.clone()).await;
    assert_eq!(first["outcome"], "recorded");
    let (_, second) = inbound(&app, msg).await;
    assert_eq!(second["outcome"], "duplicate", "the same provider ref is ignored");

    // Exactly one inbound row, not two.
    let rows = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM inbound_messages WHERE guest_id = $1"#,
        guest_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rows, 1);
}

#[sqlx::test]
async fn the_webhook_requires_its_secret_when_configured(pool: PgPool) {
    // Build an app whose config sets a webhook secret.
    let mut config = common::test_config();
    config.webhook_secret = Some("s3cret".into());
    let app = common::app_with_config(pool.clone(), config);
    seed_user(&pool, "o@example.com").await;

    // No secret header: rejected.
    let (status, _, _) = send(
        &app,
        Method::POST,
        "/api/webhooks/inbound",
        Some(json!({ "channel": "sms", "fromPhone": "08000000000", "body": "1" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// --- passes -------------------------------------------------------------------

#[sqlx::test]
async fn confirming_hands_the_guest_their_passes(pool: PgPool) {
    let (app, guest_id, token, [_, reception]) = seed(&pool).await;

    let (_, before) = get_rsvp(&app, token).await;
    assert!(
        !before.contains("Your pass"),
        "nothing to show before they've answered"
    );

    let form = format!("attending={reception}&party_{reception}=2");
    assert_eq!(post_rsvp(&app, token, &form).await, StatusCode::SEE_OTHER);

    let (status, body) = get_rsvp(&app, token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Your passes"), "two heads, so plural");
    assert!(
        body.contains("Aunt Ngozi +1"),
        "the plus-one gets their own, labelled by index"
    );
    assert!(
        !body.contains("Aunt Ngozi +2"),
        "they said two are coming, so two squares — not four"
    );

    // Each pass points at the image endpoint with that head's real code.
    let issued: Vec<String> = sqlx::query_scalar!(
        "SELECT code FROM attendees WHERE guest_id = $1 ORDER BY head_index",
        guest_id
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(issued.len(), 4, "codes exist for every head they may bring");
    for code in &issued[..2] {
        assert!(body.contains(&format!("/q/{code}")), "the square for {code}");
        // The readable fallback, spaced the way it gets read aloud.
        assert!(
            body.contains(&format!("{} {}", &code[..4], &code[4..])),
            "the spoken form of {code}"
        );
    }
}

#[sqlx::test]
async fn changing_to_a_decline_takes_the_passes_back(pool: PgPool) {
    let (app, _, token, [_, reception]) = seed(&pool).await;

    let form = format!("attending={reception}&party_{reception}=1");
    post_rsvp(&app, token, &form).await;
    let (_, body) = get_rsvp(&app, token).await;
    assert!(body.contains("Your pass"));

    post_rsvp(&app, token, "").await;
    let (_, body) = get_rsvp(&app, token).await;
    assert!(
        !body.contains("Your pass"),
        "someone who isn't coming shouldn't be holding a pass"
    );
}

#[sqlx::test]
async fn the_qr_endpoint_draws_a_code_without_confirming_who_holds_it(pool: PgPool) {
    let (app, _, token, [_, reception]) = seed(&pool).await;
    post_rsvp(&app, token, &format!("attending={reception}&party_{reception}=1")).await;

    let code: String =
        sqlx::query_scalar!("SELECT code FROM attendees ORDER BY head_index LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();

    let (status, body, headers) = common::get_html(&app, &format!("/q/{code}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[axum::http::header::CONTENT_TYPE], "image/svg+xml");
    assert!(body.contains("<svg"), "an SVG square");

    // A code that was never issued still renders, so the endpoint can't be used
    // to find out who is on the guest list.
    let (status, _, _) = common::get_html(&app, "/q/ACHD4F7K").await;
    assert_eq!(status, StatusCode::OK);

    // Anything that isn't shaped like one of our codes is simply not an image.
    for junk in ["nonsense", "ACHD4F7", "WIFI"] {
        let (status, _, _) = common::get_html(&app, &format!("/q/{junk}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{junk}");
    }
}
