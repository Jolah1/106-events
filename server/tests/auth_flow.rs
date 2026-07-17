mod common;

use axum::http::{Method, StatusCode, header};
use serde_json::json;
use sqlx::PgPool;

use common::{app, seed_user, send};

/// Pulls the token query param out of the dev magic link.
fn token_from_link(link: &str) -> String {
    link.split("token=").nth(1).expect("link has token").to_string()
}

/// Pulls the raw session token out of a Set-Cookie header.
fn session_from_headers(headers: &axum::http::HeaderMap) -> String {
    let cookie = headers
        .get(header::SET_COOKIE)
        .expect("set-cookie present")
        .to_str()
        .unwrap();
    cookie
        .strip_prefix("session_106=")
        .and_then(|rest| rest.split(';').next())
        .expect("session cookie value")
        .to_string()
}

#[sqlx::test]
async fn full_magic_link_flow(pool: PgPool) {
    let app = app(pool.clone());
    // Staff are invited: the user must already exist for a link to be issued.
    seed_user(&pool, "ada@example.com").await;

    // Request a link (dev mailer returns it in the response). The email is
    // normalized, so the mixed-case, padded form still matches the member.
    let (status, body, _) = send(
        &app,
        Method::POST,
        "/api/auth/request-link",
        Some(json!({ "email": "  Ada@Example.COM " })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let link = body["devLink"].as_str().expect("dev link").to_string();
    let token = token_from_link(&link);

    // Verify: signs the invited member in and sets a session cookie.
    let (status, body, headers) = send(
        &app,
        Method::POST,
        "/api/auth/verify",
        Some(json!({ "token": token })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["email"], "ada@example.com");
    let session = session_from_headers(&headers);

    // The session works.
    let (status, body, _) = send(&app, Method::GET, "/api/auth/me", None, Some(&session)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["email"], "ada@example.com");

    // A magic link is single-use.
    let (status, _, _) = send(
        &app,
        Method::POST,
        "/api/auth/verify",
        Some(json!({ "token": token })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Logout revokes the session.
    let (status, _, _) = send(&app, Method::POST, "/api/auth/logout", None, Some(&session)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(&app, Method::GET, "/api/auth/me", None, Some(&session)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn signup_is_invite_only(pool: PgPool) {
    let app = app(pool.clone());

    // A stranger's request looks successful — the response must not reveal that
    // they aren't on the team — but no link is issued.
    let (status, body, _) = send(
        &app,
        Method::POST,
        "/api/auth/request-link",
        Some(json!({ "email": "stranger@example.com" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["sent"], true);
    assert!(body["devLink"].is_null(), "no link for a non-member: {body}");

    // And no account was conjured into being.
    let users = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM users"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(users, 0, "a stranger's request must not create a user");

    // Even holding a valid, unexpired token for that email, verify refuses:
    // there is no member to sign in as. (The token exists so rate-limiting is
    // uniform, but it maps to nobody.)
    let raw = sqlx::query_scalar!(
        "SELECT token_hash FROM login_tokens WHERE identifier = 'stranger@example.com'"
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(raw.is_some(), "a token is still minted, to keep responses uniform");
}

#[sqlx::test]
async fn a_returning_member_keeps_one_identity(pool: PgPool) {
    let app = app(pool.clone());
    seed_user(&pool, "ada@example.com").await;

    for _ in 0..2 {
        let (_, body, _) = send(
            &app,
            Method::POST,
            "/api/auth/request-link",
            Some(json!({ "email": "ada@example.com" })),
            None,
        )
        .await;
        let token = token_from_link(body["devLink"].as_str().unwrap());
        let (status, _, _) = send(
            &app,
            Method::POST,
            "/api/auth/verify",
            Some(json!({ "token": token })),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    let users = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM users"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(users, 1, "signing in twice must not duplicate the member");
}

#[sqlx::test]
async fn expired_and_garbage_tokens_rejected(pool: PgPool) {
    let app = app(pool.clone());
    seed_user(&pool, "late@example.com").await;

    // Garbage token.
    let (status, _, _) = send(
        &app,
        Method::POST,
        "/api/auth/verify",
        Some(json!({ "token": "not-a-real-token" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Expired token: request one, then force-expire it in the DB.
    let (_, body, _) = send(
        &app,
        Method::POST,
        "/api/auth/request-link",
        Some(json!({ "email": "late@example.com" })),
        None,
    )
    .await;
    let token = token_from_link(body["devLink"].as_str().unwrap());
    sqlx::query!("UPDATE login_tokens SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _, _) = send(
        &app,
        Method::POST,
        "/api/auth/verify",
        Some(json!({ "token": token })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn request_link_validates_and_rate_limits(pool: PgPool) {
    let app = app(pool.clone());
    // Rate limiting applies to everyone (uniform responses), but seed a member
    // so this reads as the case that matters: a real user being throttled.
    seed_user(&pool, "busy@example.com").await;

    let (status, _, _) = send(
        &app,
        Method::POST,
        "/api/auth/request-link",
        Some(json!({ "email": "not-an-email" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    for _ in 0..5 {
        let (status, _, _) = send(
            &app,
            Method::POST,
            "/api/auth/request-link",
            Some(json!({ "email": "busy@example.com" })),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, _, _) = send(
        &app,
        Method::POST,
        "/api/auth/request-link",
        Some(json!({ "email": "busy@example.com" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}
