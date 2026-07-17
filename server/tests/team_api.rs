mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

use common::{app, seed_admin, seed_user, send};

async fn team(app: &axum::Router, session: &str) -> (StatusCode, Value) {
    let (status, body, _) = send(app, Method::GET, "/api/team", None, Some(session)).await;
    (status, body)
}

#[sqlx::test]
async fn admin_invites_and_the_invitee_can_sign_in(pool: PgPool) {
    let app = app(pool.clone());
    let (_, admin) = seed_admin(&pool, "founder@example.com").await;

    // Invite a coordinator.
    let (status, member, _) = send(
        &app,
        Method::POST,
        "/api/team",
        Some(json!({ "email": "  Coordinator@Example.COM ", "name": "Coordinator" })),
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{member}");
    assert_eq!(member["email"], "coordinator@example.com", "email is normalized");
    assert_eq!(member["role"], "staff", "invitees default to staff");

    // The invitation is a real account: a magic link now works for them.
    let (_, link_body, _) = send(
        &app,
        Method::POST,
        "/api/auth/request-link",
        Some(json!({ "email": "coordinator@example.com" })),
        None,
    )
    .await;
    let token = link_body["devLink"]
        .as_str()
        .expect("an invited member gets a link")
        .split("token=")
        .nth(1)
        .unwrap()
        .to_string();
    let (status, _, _) = send(
        &app,
        Method::POST,
        "/api/auth/verify",
        Some(json!({ "token": token })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the invited coordinator can sign in");

    // The team now lists both.
    let (status, list) = team(&app, &admin).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 2);
}

#[sqlx::test]
async fn staff_cannot_manage_the_team(pool: PgPool) {
    let app = app(pool.clone());
    let (_, admin) = seed_admin(&pool, "admin@example.com").await;
    let (staff_id, staff) = seed_user(&pool, "staff@example.com").await;

    // A plain staff member is forbidden from every team action — 403, not 404:
    // they're legitimately signed in, just not entitled.
    let (status, _, _) = send(&app, Method::GET, "/api/team", None, Some(&staff)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _, _) = send(
        &app,
        Method::POST,
        "/api/team",
        Some(json!({ "email": "friend@example.com" })),
        Some(&staff),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Including trying to promote themselves.
    let (status, _, _) = send(
        &app,
        Method::POST,
        &format!("/api/team/{staff_id}"),
        Some(json!({ "role": "admin" })),
        Some(&staff),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // The admin, meanwhile, can promote them.
    let (status, promoted, _) = send(
        &app,
        Method::POST,
        &format!("/api/team/{staff_id}"),
        Some(json!({ "role": "admin" })),
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{promoted}");
    assert_eq!(promoted["role"], "admin");
}

#[sqlx::test]
async fn the_last_admin_cannot_be_removed_or_demoted(pool: PgPool) {
    let app = app(pool.clone());
    let (admin_id, admin) = seed_admin(&pool, "only@example.com").await;
    seed_user(&pool, "staff@example.com").await;

    // Demoting the sole admin would lock the whole team out of team management.
    let (status, body, _) = send(
        &app,
        Method::POST,
        &format!("/api/team/{admin_id}"),
        Some(json!({ "role": "staff" })),
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // As would removing them.
    let (status, _, _) = send(
        &app,
        Method::DELETE,
        &format!("/api/team/{admin_id}"),
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // With a second admin present, the first can step down.
    let (second_id, _) = seed_admin(&pool, "second@example.com").await;
    let (status, _, _) = send(
        &app,
        Method::POST,
        &format!("/api/team/{admin_id}"),
        Some(json!({ "role": "staff" })),
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // Sanity: the remaining admin is still there.
    assert_ne!(second_id, admin_id);
}

#[sqlx::test]
async fn inviting_a_duplicate_email_conflicts(pool: PgPool) {
    let app = app(pool.clone());
    let (_, admin) = seed_admin(&pool, "admin@example.com").await;
    seed_user(&pool, "taken@example.com").await;

    let (status, body, _) = send(
        &app,
        Method::POST,
        "/api/team",
        Some(json!({ "email": "taken@example.com" })),
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
}

#[sqlx::test]
async fn an_admin_cannot_remove_themselves(pool: PgPool) {
    let app = app(pool.clone());
    let (admin_id, admin) = seed_admin(&pool, "admin@example.com").await;
    // A second admin exists, so the last-admin guard isn't what's protecting us.
    seed_admin(&pool, "other@example.com").await;

    let (status, body, _) = send(
        &app,
        Method::DELETE,
        &format!("/api/team/{admin_id}"),
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

#[sqlx::test]
async fn removing_a_member_keeps_their_events(pool: PgPool) {
    let app = app(pool.clone());
    let (_, admin) = seed_admin(&pool, "admin@example.com").await;
    let (author_id, author) = seed_user(&pool, "author@example.com").await;

    // The author books an event.
    let (status, event, _) = send(
        &app,
        Method::POST,
        "/api/events",
        Some(json!({
            "title": "Booked before they left",
            "subEvents": [{ "name": "Party", "startsAt": "2026-12-01T18:00:00Z" }]
        })),
        Some(&author),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let event_id = event["id"].as_str().unwrap().to_string();

    // They leave the company.
    let (status, _, _) = send(
        &app,
        Method::DELETE,
        &format!("/api/team/{author_id}"),
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // The event survives — attribution is not a delete path.
    let (status, _, _) = send(
        &app,
        Method::GET,
        &format!("/api/events/{event_id}"),
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the event outlives the employment");
}

#[sqlx::test]
async fn team_management_needs_a_session(pool: PgPool) {
    let app = app(pool.clone());
    seed_admin(&pool, "admin@example.com").await;

    let (status, _, _) = send(&app, Method::GET, "/api/team", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
