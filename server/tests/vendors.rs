mod common;

use axum::http::{Method, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use common::{app, seed_user, send};

/// Naira as kobo. Money crosses the wire as an integer number of kobo, and
/// spelling that out here keeps the amounts readable as naira.
const fn naira(amount: i64) -> i64 {
    amount * 100
}

struct Seeded {
    app: axum::Router,
    session: String,
    event_id: Uuid,
}

async fn seed(pool: &PgPool) -> Seeded {
    let app = app(pool.clone());
    let (_, session) = seed_user(pool, "organizer@example.com").await;
    let (_, event, _) = send(
        &app,
        Method::POST,
        "/api/events",
        Some(json!({
            "title": "Ada & Tunde",
            "subEvents": [{ "name": "Reception", "startsAt": "2099-11-21T13:00:00Z" }]
        })),
        Some(&session),
    )
    .await;
    Seeded {
        app,
        session,
        event_id: event["id"].as_str().unwrap().parse().unwrap(),
    }
}

#[sqlx::test]
async fn a_vendor_is_tracked_with_its_money(pool: PgPool) {
    let s = seed(&pool).await;

    let (status, vendor, _) = send(
        &s.app,
        Method::POST,
        &format!("/api/events/{}/vendors", s.event_id),
        Some(json!({
            "name": "Ronke's Kitchen",
            "category": "Catering",
            "phone": "0806 688 2563",
            "service": "Small chops and jollof for 300",
            "costKobo": naira(150_000),
            "amountPaidKobo": naira(50_000),
            "notes": "Deposit paid by transfer"
        })),
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{vendor}");
    assert_eq!(vendor["name"], "Ronke's Kitchen");
    assert_eq!(vendor["phone"], "+2348066882563", "normalized like a guest's");
    assert_eq!(vendor["paidStatus"], "part_paid");
    assert_eq!(vendor["outstandingKobo"], naira(100_000));
}

#[sqlx::test]
async fn paid_status_is_derived_so_it_cannot_drift(pool: PgPool) {
    let s = seed(&pool).await;
    let (_, vendor, _) = send(
        &s.app,
        Method::POST,
        &format!("/api/events/{}/vendors", s.event_id),
        Some(json!({ "name": "DJ Spinall", "costKobo": naira(300_000) })),
        Some(&s.session),
    )
    .await;
    let id = vendor["id"].as_str().unwrap();
    assert_eq!(vendor["paidStatus"], "unpaid");

    // A deposit.
    let (_, vendor, _) = send(
        &s.app,
        Method::PATCH,
        &format!("/api/vendors/{id}"),
        Some(json!({ "amountPaidKobo": naira(100_000) })),
        Some(&s.session),
    )
    .await;
    assert_eq!(vendor["paidStatus"], "part_paid");
    assert_eq!(vendor["outstandingKobo"], naira(200_000));

    // Settled.
    let (_, vendor, _) = send(
        &s.app,
        Method::PATCH,
        &format!("/api/vendors/{id}"),
        Some(json!({ "amountPaidKobo": naira(300_000) })),
        Some(&s.session),
    )
    .await;
    assert_eq!(vendor["paidStatus"], "paid");
    assert_eq!(vendor["outstandingKobo"], 0);

    // The cost is renegotiated *down* after paying in full. Status follows the
    // money rather than sticking at "paid".
    let (_, vendor, _) = send(
        &s.app,
        Method::PATCH,
        &format!("/api/vendors/{id}"),
        Some(json!({ "costKobo": naira(250_000) })),
        Some(&s.session),
    )
    .await;
    assert_eq!(vendor["paidStatus"], "overpaid", "they're owed a refund, not settled");
    assert_eq!(vendor["outstandingKobo"], 0, "an overpayment is never a negative debt");
}

#[sqlx::test]
async fn money_is_rejected_when_it_makes_no_sense(pool: PgPool) {
    let s = seed(&pool).await;
    for bad in [
        json!({ "name": "Negative", "costKobo": -100 }),
        json!({ "name": "Negative paid", "amountPaidKobo": -1 }),
        json!({ "name": "Absurd", "costKobo": 9_000_000_000_000_i64 }),
        json!({ "name": "" }),
    ] {
        let (status, _, _) = send(
            &s.app,
            Method::POST,
            &format!("/api/events/{}/vendors", s.event_id),
            Some(bad.clone()),
            Some(&s.session),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }
}

#[sqlx::test]
async fn a_patch_leaves_untouched_fields_alone(pool: PgPool) {
    let s = seed(&pool).await;
    let (_, vendor, _) = send(
        &s.app,
        Method::POST,
        &format!("/api/events/{}/vendors", s.event_id),
        Some(json!({
            "name": "Balmoral Hall",
            "category": "Venue",
            "phone": "08066882563",
            "notes": "Confirmed for the 21st",
            "costKobo": naira(800_000)
        })),
        Some(&s.session),
    )
    .await;
    let id = vendor["id"].as_str().unwrap();

    // Updating only the paid amount must not wipe the notes or the phone —
    // this is hand-typed information nobody wants to re-enter.
    let (_, updated, _) = send(
        &s.app,
        Method::PATCH,
        &format!("/api/vendors/{id}"),
        Some(json!({ "amountPaidKobo": naira(400_000) })),
        Some(&s.session),
    )
    .await;
    assert_eq!(updated["notes"], "Confirmed for the 21st");
    assert_eq!(updated["phone"], "+2348066882563");
    assert_eq!(updated["category"], "Venue");

    // An explicit null clears, which is different from omitting.
    let (_, cleared, _) = send(
        &s.app,
        Method::PATCH,
        &format!("/api/vendors/{id}"),
        Some(json!({ "phone": null })),
        Some(&s.session),
    )
    .await;
    assert!(cleared["phone"].is_null(), "{cleared}");
    assert_eq!(cleared["notes"], "Confirmed for the 21st", "still untouched");
}

#[sqlx::test]
async fn vendors_are_listed_per_event_and_shared_across_staff(pool: PgPool) {
    let s = seed(&pool).await;
    // A second event must not see the first's vendors.
    let (_, other, _) = send(
        &s.app,
        Method::POST,
        "/api/events",
        Some(json!({
            "title": "Someone Else's Party",
            "subEvents": [{ "name": "Party", "startsAt": "2099-12-01T18:00:00Z" }]
        })),
        Some(&s.session),
    )
    .await;
    let other_id = other["id"].as_str().unwrap();

    for name in ["Zara Decor", "Ade Photography"] {
        send(
            &s.app,
            Method::POST,
            &format!("/api/events/{}/vendors", s.event_id),
            Some(json!({ "name": name })),
            Some(&s.session),
        )
        .await;
    }
    send(
        &s.app,
        Method::POST,
        &format!("/api/events/{other_id}/vendors"),
        Some(json!({ "name": "Wrong Event Caterer" })),
        Some(&s.session),
    )
    .await;

    // A colleague sees the same sheet: 106 Events is one workspace.
    let (_, colleague) = seed_user(&pool, "coordinator@example.com").await;
    let (status, list, _) = send(
        &s.app,
        Method::GET,
        &format!("/api/events/{}/vendors", s.event_id),
        None,
        Some(&colleague),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Ade Photography", "Zara Decor"], "alphabetical, this event only");
}

#[sqlx::test]
async fn deleting_an_event_takes_its_vendor_sheet(pool: PgPool) {
    let s = seed(&pool).await;
    send(
        &s.app,
        Method::POST,
        &format!("/api/events/{}/vendors", s.event_id),
        Some(json!({ "name": "Ronke's Kitchen", "costKobo": naira(100) })),
        Some(&s.session),
    )
    .await;

    let (status, _, _) = send(
        &s.app,
        Method::DELETE,
        &format!("/api/events/{}", s.event_id),
        None,
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let left = sqlx::query_scalar!(r#"SELECT count(*) AS "c!" FROM vendors"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0, "the sheet goes with the event");
}

#[sqlx::test]
async fn a_vendor_can_be_removed(pool: PgPool) {
    let s = seed(&pool).await;
    let (_, vendor, _) = send(
        &s.app,
        Method::POST,
        &format!("/api/events/{}/vendors", s.event_id),
        Some(json!({ "name": "Cancelled Band" })),
        Some(&s.session),
    )
    .await;
    let id = vendor["id"].as_str().unwrap();

    let (status, _, _) = send(
        &s.app,
        Method::DELETE,
        &format!("/api/vendors/{id}"),
        None,
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _, _) = send(
        &s.app,
        Method::DELETE,
        &format!("/api/vendors/{id}"),
        None,
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "already gone");
}

#[sqlx::test]
async fn the_vendor_sheet_needs_a_session(pool: PgPool) {
    let s = seed(&pool).await;
    let (status, _, _) = send(
        &s.app,
        Method::GET,
        &format!("/api/events/{}/vendors", s.event_id),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // And an unknown event is a 404 rather than an empty sheet.
    let (status, _, _) = send(
        &s.app,
        Method::GET,
        &format!("/api/events/{}/vendors", Uuid::new_v4()),
        None,
        Some(&s.session),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
