// Each integration test binary compiles this module and uses a subset of it.
#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use server::{
    auth::tokens,
    config::Config,
    mailer::Mailer,
    routes,
    state::AppState,
};

pub fn test_config() -> Config {
    Config {
        database_url: String::new(), // unused: the pool is injected
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        app_base_url: "http://localhost:5173".into(),
        public_base_url: "http://localhost:8080".into(),
        smtp_url: None,
        email_from: "test@example.com".into(),
        cookie_secure: false,
        session_ttl_days: 30,
        login_token_ttl_minutes: 15,
        admin_emails: Vec::new(),
        dashboard_dist: None,
    }
}

pub fn app(pool: PgPool) -> Router {
    routes::router(AppState {
        pool,
        config: Arc::new(test_config()),
        mailer: Arc::new(Mailer::DevLog),
    })
}

/// Sends a JSON request, returning status, parsed body (Null when empty) and
/// response headers.
pub async fn send(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    session: Option<&str>,
) -> (StatusCode, Value, HeaderMap) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = session {
        builder = builder.header(header::COOKIE, format!("session_106={token}"));
    }
    let request = match body {
        Some(json) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string())),
        None => builder.body(Body::empty()),
    }
    .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value, headers)
}

/// GETs a page and returns status, body as text, and response headers.
/// Public pages render HTML, so `send`'s JSON parsing is useless for them.
pub async fn get_html(app: &Router, uri: &str) -> (StatusCode, String, HeaderMap) {
    let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned(), headers)
}

/// Creates a staff user + session directly in the DB; returns (user_id, raw
/// session token) for authenticated requests.
pub async fn seed_user(pool: &PgPool, email: &str) -> (Uuid, String) {
    seed_member(pool, email, "staff").await
}

/// Creates an admin user + session.
pub async fn seed_admin(pool: &PgPool, email: &str) -> (Uuid, String) {
    seed_member(pool, email, "admin").await
}

pub async fn seed_member(pool: &PgPool, email: &str, role: &str) -> (Uuid, String) {
    let user_id = sqlx::query_scalar!(
        "INSERT INTO users (email, role) VALUES ($1, $2) RETURNING id",
        email,
        role
    )
    .fetch_one(pool)
    .await
    .unwrap();

    let token = tokens::generate_token();
    sqlx::query!(
        "INSERT INTO sessions (user_id, token_hash, expires_at)
         VALUES ($1, $2, now() + interval '1 day')",
        user_id,
        token.hash
    )
    .execute(pool)
    .await
    .unwrap();

    (user_id, token.raw)
}
