//! The landing page and the queue of people asking to be let in.

mod common;

use axum::http::{Method, StatusCode, header};
use serde_json::json;
use sqlx::PgPool;

use common::{app, get_html, seed_member, seed_user, send};

/// Posts the request-access form the way a browser does.
async fn post_form(app: &axum::Router, body: &str) -> (StatusCode, String) {
    use tower::ServiceExt;
    let request = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/request-access")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[sqlx::test]
async fn the_landing_page_says_what_the_product_does(pool: PgPool) {
    let app = app(pool.clone());
    let (status, body, headers) = get_html(&app, "/").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("One guest list."), "the headline");
    assert!(body.contains("Work with us"), "the way in");
    assert!(
        body.contains("/q/MTXN9RGJ"),
        "the example pass points at the real QR endpoint"
    );
    // It's the same for everyone, so it may be cached — but briefly, because
    // the copy changes as the product does.
    assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=300");
}

#[sqlx::test]
async fn the_landing_page_needs_no_account(pool: PgPool) {
    // No user, no session, no cookie: a stranger arriving from a search result.
    let app = app(pool.clone());
    let (status, _, _) = get_html(&app, "/").await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn asking_for_an_account_queues_the_request(pool: PgPool) {
    let app = app(pool.clone());

    let (status, _) = post_form(
        &app,
        "name=Bimpe+Adewale&email=Bimpe%40Planner.NG&phone=0806+688+2563\
         &about=Wedding+in+November&budget=%E2%82%A63m+-+%E2%82%A65m",
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "post/redirect/get");

    let row =
        sqlx::query!("SELECT name, email, phone, about, budget, handled_at FROM access_requests")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.name, "Bimpe Adewale");
    assert_eq!(row.email, "bimpe@planner.ng", "stored lowercase");
    assert_eq!(
        row.phone.as_deref(),
        Some("+2348066882563"),
        "normalized like a guest's, so nobody retypes it to call them"
    );
    assert_eq!(row.about, "Wedding in November");
    assert_eq!(row.budget, "₦3m - ₦5m", "kept in their own words");
    assert!(row.handled_at.is_none(), "it starts open");

    // The page then says so, without needing any state beyond the URL.
    let (_, body, _) = get_html(&app, "/?requested=true").await;
    assert!(body.contains("we'll be in touch"));
}

#[sqlx::test]
async fn asking_twice_reopens_the_same_request(pool: PgPool) {
    let app = app(pool.clone());
    post_form(&app, "name=Bimpe&email=bimpe%40planner.ng&phone=08066882563").await;

    // An admin deals with it...
    sqlx::query!("UPDATE access_requests SET handled_at = now()")
        .execute(&pool)
        .await
        .unwrap();

    // ...and they come back, in different case and without the number.
    let (status, _) = post_form(
        &app,
        "name=Bimpe+Adewale&email=BIMPE%40PLANNER.NG&about=Following+up",
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let rows = sqlx::query!("SELECT name, phone, about, handled_at FROM access_requests")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "one person, one row in the queue");
    assert_eq!(rows[0].about, "Following up", "the latest words win");
    assert_eq!(
        rows[0].phone.as_deref(),
        Some("+2348066882563"),
        "a number they gave once isn't lost by not repeating it"
    );
    assert!(
        rows[0].handled_at.is_none(),
        "knocking again puts them back in front of an admin"
    );
}

#[sqlx::test]
async fn a_bad_email_comes_back_with_the_typing_intact(pool: PgPool) {
    let app = app(pool.clone());
    let (status, body) = post_form(&app, "name=Bimpe+Adewale&email=not-an-email").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("enter a valid email address"));
    assert!(
        body.contains("value=\"Bimpe Adewale\""),
        "nobody should have to retype what they already wrote"
    );
    let count = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM access_requests"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test]
async fn a_bot_that_fills_the_honeypot_is_answered_and_ignored(pool: PgPool) {
    let app = app(pool.clone());
    let (status, _) = post_form(
        &app,
        "name=Bot&email=bot%40spam.example&website=http%3A%2F%2Fspam.example",
    )
    .await;

    // Indistinguishable from success, so it learns nothing about the check.
    assert_eq!(status, StatusCode::SEE_OTHER);
    let count = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM access_requests"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "and nothing reaches the queue");
}

#[sqlx::test]
async fn the_queue_is_for_admins_only(pool: PgPool) {
    let app = app(pool.clone());
    post_form(&app, "name=Bimpe&email=bimpe%40planner.ng").await;

    let (_, staff_session) = seed_user(&pool, "staff@example.com").await;
    let (status, _, _) = send(&app, Method::GET, "/api/access-requests", None, Some(&staff_session))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "staff can't read who's asking");

    let (status, _, _) = send(&app, Method::GET, "/api/access-requests", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (_, admin_session) = seed_member(&pool, "admin@example.com", "admin").await;
    let (status, list, _) =
        send(&app, Method::GET, "/api/access-requests", None, Some(&admin_session)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["email"], "bimpe@planner.ng");
}

#[sqlx::test]
async fn dealing_with_a_request_clears_it_from_the_queue(pool: PgPool) {
    let app = app(pool.clone());
    post_form(&app, "name=Bimpe&email=bimpe%40planner.ng").await;
    let (_, session) = seed_member(&pool, "admin@example.com", "admin").await;

    let (_, list, _) =
        send(&app, Method::GET, "/api/access-requests", None, Some(&session)).await;
    let id = list[0]["id"].as_str().unwrap().to_string();

    let (status, handled, _) = send(
        &app,
        Method::POST,
        &format!("/api/access-requests/{id}/handled"),
        None,
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(handled["handledAt"].is_string());

    let (_, list, _) =
        send(&app, Method::GET, "/api/access-requests", None, Some(&session)).await;
    assert!(list.as_array().unwrap().is_empty(), "the queue only shows open ones");

    // Inviting them is the other half of the same job, and it still works after.
    let (status, _, _) = send(
        &app,
        Method::POST,
        "/api/team",
        Some(json!({ "email": "bimpe@planner.ng", "name": "Bimpe" })),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}
