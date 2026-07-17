mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

use common::{app, seed_user, send};

fn wedding_body() -> Value {
    json!({
        "title": "Adaeze & Tunde 2026",
        "description": "Join us as we celebrate.",
        "timezone": "Africa/Lagos",
        "subEvents": [
            {
                "name": "Traditional Engagement",
                "startsAt": "2026-11-20T10:00:00Z",
                "venueName": "The Haven, Ikeja"
            },
            {
                "name": "White Wedding",
                "startsAt": "2026-11-21T09:00:00Z",
                "venueName": "Our Saviour's Church"
            },
            {
                "name": "Reception",
                "startsAt": "2026-11-21T13:00:00Z",
                "endsAt": "2026-11-21T20:00:00Z",
                "venueName": "Balmoral Hall, VI"
            }
        ]
    })
}

#[sqlx::test]
async fn create_and_fetch_event_with_sub_events(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;

    let (status, body, _) = send(
        &app,
        Method::POST,
        "/api/events",
        Some(wedding_body()),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["slug"], "adaeze-tunde-2026");
    assert_eq!(body["subEvents"].as_array().unwrap().len(), 3);
    assert_eq!(body["subEvents"][0]["slug"], "traditional-engagement");
    let event_id = body["id"].as_str().unwrap().to_string();

    // List shows the rollup.
    let (status, body, _) = send(&app, Method::GET, "/api/events", None, Some(&session)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["subEventCount"], 3);
    assert_eq!(body[0]["firstStartsAt"], "2026-11-20T10:00:00Z");

    // Detail keeps sub-events in position order.
    let (status, body, _) = send(
        &app,
        Method::GET,
        &format!("/api/events/{event_id}"),
        None,
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = body["subEvents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Traditional Engagement", "White Wedding", "Reception"]);
}

#[sqlx::test]
async fn duplicate_titles_get_distinct_slugs(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;

    let mut slugs = Vec::new();
    for _ in 0..2 {
        let (status, body, _) = send(
            &app,
            Method::POST,
            "/api/events",
            Some(wedding_body()),
            Some(&session),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        slugs.push(body["slug"].as_str().unwrap().to_string());
    }
    assert_ne!(slugs[0], slugs[1]);
    assert!(slugs[1].starts_with("adaeze-tunde-2026-"));
}

#[sqlx::test]
async fn validation_rejects_bad_input(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;

    // No sub-events.
    let (status, _, _) = send(
        &app,
        Method::POST,
        "/api/events",
        Some(json!({ "title": "Empty", "subEvents": [] })),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Ends before it starts.
    let (status, _, _) = send(
        &app,
        Method::POST,
        "/api/events",
        Some(json!({
            "title": "Backwards",
            "subEvents": [{
                "name": "Party",
                "startsAt": "2026-11-21T10:00:00Z",
                "endsAt": "2026-11-21T09:00:00Z"
            }]
        })),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Unknown timezone.
    let (status, _, _) = send(
        &app,
        Method::POST,
        "/api/events",
        Some(json!({
            "title": "Nowhere",
            "timezone": "Mars/Olympus_Mons",
            "subEvents": [{ "name": "Launch", "startsAt": "2026-11-21T10:00:00Z" }]
        })),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn events_are_shared_across_the_workspace(pool: PgPool) {
    let app = app(pool.clone());
    // Two staff members of the one agency. 106 Events is a single workspace:
    // whoever books an event, any colleague can work it.
    let (_, founder) = seed_user(&pool, "founder@example.com").await;
    let (_, coordinator) = seed_user(&pool, "coordinator@example.com").await;

    let (_, body, _) = send(
        &app,
        Method::POST,
        "/api/events",
        Some(wedding_body()),
        Some(&founder),
    )
    .await;
    let event_id = body["id"].as_str().unwrap().to_string();
    let sub_event_id = body["subEvents"][0]["id"].as_str().unwrap().to_string();

    // The coordinator sees the founder's event in the list...
    let (status, list, _) = send(&app, Method::GET, "/api/events", None, Some(&coordinator)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1, "the workspace's events are shared");

    // ...and can open, edit, and edit the parts of it.
    let uri = format!("/api/events/{event_id}");
    let (status, _, _) = send(&app, Method::GET, &uri, None, Some(&coordinator)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, edited, _) = send(
        &app,
        Method::PATCH,
        &uri,
        Some(json!({ "title": "Renamed by a colleague" })),
        Some(&coordinator),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(edited["title"], "Renamed by a colleague");
    let (status, _, _) = send(
        &app,
        Method::PATCH,
        &format!("/api/sub-events/{sub_event_id}"),
        Some(json!({ "name": "Retimed" })),
        Some(&coordinator),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // A stranger with no session still gets nothing.
    let (status, _, _) = send(&app, Method::GET, "/api/events", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = send(&app, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn sub_event_lifecycle(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;

    // Simple event: a single default sub-event.
    let (_, body, _) = send(
        &app,
        Method::POST,
        "/api/events",
        Some(json!({
            "title": "Housewarming",
            "subEvents": [{
                "name": "Housewarming",
                "startsAt": "2026-09-05T15:00:00Z",
                "isDefault": true
            }]
        })),
        Some(&session),
    )
    .await;
    let event_id = body["id"].as_str().unwrap().to_string();
    let only_sub = body["subEvents"][0]["id"].as_str().unwrap().to_string();

    // The last sub-event can't be deleted.
    let (status, body, _) = send(
        &app,
        Method::DELETE,
        &format!("/api/sub-events/{only_sub}"),
        None,
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // Add a second part, then the first becomes deletable.
    let (status, added, _) = send(
        &app,
        Method::POST,
        &format!("/api/events/{event_id}/sub-events"),
        Some(json!({ "name": "After Party", "startsAt": "2026-09-05T21:00:00Z" })),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(added["position"], 1);

    // Update it.
    let sub_id = added["id"].as_str().unwrap();
    let (status, updated, _) = send(
        &app,
        Method::PATCH,
        &format!("/api/sub-events/{sub_id}"),
        Some(json!({ "name": "Owambe After Party", "venueName": "Rooftop" })),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], "Owambe After Party");
    assert_eq!(updated["venueName"], "Rooftop");
    assert_eq!(updated["slug"], "after-party", "slug is stable across renames");

    // endsAt distinguishes absent (keep) from null (clear).
    let (status, updated, _) = send(
        &app,
        Method::PATCH,
        &format!("/api/sub-events/{sub_id}"),
        Some(json!({ "endsAt": "2026-09-06T02:00:00Z" })),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["endsAt"], "2026-09-06T02:00:00Z");

    let (status, updated, _) = send(
        &app,
        Method::PATCH,
        &format!("/api/sub-events/{sub_id}"),
        Some(json!({ "venueAddress": "12 Marina Rd" })),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["endsAt"], "2026-09-06T02:00:00Z", "absent endsAt keeps value");

    let (status, updated, _) = send(
        &app,
        Method::PATCH,
        &format!("/api/sub-events/{sub_id}"),
        Some(json!({ "endsAt": null })),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(updated["endsAt"].is_null(), "null endsAt clears value: {updated}");

    let (status, _, _) = send(
        &app,
        Method::DELETE,
        &format!("/api/sub-events/{only_sub}"),
        None,
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}
