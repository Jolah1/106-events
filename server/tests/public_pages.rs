mod common;

use axum::http::{Method, StatusCode, header};
use common::{app, get_html, seed_user, send};
use serde_json::json;
use sqlx::PgPool;

/// Creates an event via the API and returns its public slug.
async fn seed_event(app: &axum::Router, session: &str, body: serde_json::Value) -> String {
    let (status, created, _) = send(app, Method::POST, "/api/events", Some(body), Some(session)).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    created["slug"].as_str().unwrap().to_string()
}

fn wedding() -> serde_json::Value {
    json!({
        "title": "Tolu & Emeka's Wedding",
        "description": "Join us as two families become one.",
        "timezone": "Africa/Lagos",
        "subEvents": [
            {
                "name": "Church Ceremony",
                "startsAt": "2026-11-21T09:00:00Z",
                "endsAt": "2026-11-21T11:00:00Z",
                "venueName": "Our Saviour's Church",
                "venueAddress": "12 Marina Road, Lagos"
            },
            { "name": "Reception", "startsAt": "2026-11-21T13:00:00Z" }
        ]
    })
}

#[sqlx::test]
async fn event_page_renders_schedule_in_the_event_timezone(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "organizer@example.com").await;
    let slug = seed_event(&app, &session, wedding()).await;

    let (status, html, headers) = get_html(&app, &format!("/e/{slug}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=60");

    assert!(html.contains("Tolu &#38; Emeka&#39;s Wedding"), "title is rendered: {html}");
    assert!(html.contains("Join us as two families become one."));

    // Both parts, with times converted from UTC into Africa/Lagos (+1).
    assert!(html.contains("Church Ceremony"));
    assert!(html.contains("10:00 AM – 12:00 PM"), "ceremony time in Lagos: {html}");
    assert!(html.contains("Reception"));
    assert!(html.contains("2:00 PM"), "reception time in Lagos: {html}");
    assert!(html.contains("Saturday, 21 November 2026"));

    // Venue with a maps link; the reception has none, so no stray link.
    assert!(html.contains("Our Saviour&#39;s Church"));
    assert!(html.contains("12 Marina Road, Lagos"));
    assert!(
        html.contains("query=Our%20Saviour%27s%20Church%2C%2012%20Marina%20Road%2C%20Lagos"),
        "maps query is percent-encoded: {html}"
    );
    assert_eq!(html.matches("Get directions").count(), 1);

    // No JavaScript on a guest-facing page.
    assert!(!html.contains("<script"), "public pages ship no JS");
}

#[sqlx::test]
async fn event_page_emits_link_preview_tags(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "organizer@example.com").await;
    let slug = seed_event(&app, &session, wedding()).await;

    let (_, html, _) = get_html(&app, &format!("/e/{slug}")).await;

    let expect = [
        r#"<meta property="og:type" content="website">"#.to_string(),
        r#"<meta property="og:site_name" content="106 Events">"#.to_string(),
        r#"<meta property="og:title" content="Tolu &#38; Emeka&#39;s Wedding">"#.to_string(),
        r#"<meta property="og:description" content="Join us as two families become one.">"#.to_string(),
        format!(r#"<meta property="og:url" content="http://localhost:8080/e/{slug}">"#),
        r#"<meta property="og:image" content="http://localhost:8080/static/og-default-v1.png">"#.to_string(),
        r#"<meta property="og:image:width" content="1200">"#.to_string(),
        r#"<meta property="og:image:height" content="630">"#.to_string(),
        r#"<meta name="twitter:card" content="summary_large_image">"#.to_string(),
        format!(r#"<link rel="canonical" href="http://localhost:8080/e/{slug}">"#),
    ];
    for tag in expect {
        assert!(html.contains(&tag), "missing {tag}\nin: {html}");
    }
    assert!(html.contains("<title>Tolu &#38; Emeka&#39;s Wedding · 106 Events</title>"));
}

#[sqlx::test]
async fn description_free_event_previews_with_its_date(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "organizer@example.com").await;
    let slug = seed_event(
        &app,
        &session,
        json!({
            "title": "Housewarming",
            "subEvents": [{ "name": "Housewarming", "startsAt": "2026-09-05T15:00:00Z", "isDefault": true }]
        }),
    )
    .await;

    let (_, html, _) = get_html(&app, &format!("/e/{slug}")).await;
    assert!(
        html.contains(r#"<meta property="og:description" content="Saturday, 5 September 2026">"#),
        "date stands in for a missing description: {html}"
    );
    // A lone default sub-event is an implementation detail: no "Schedule"
    // heading, and the part's name is not echoed under the event title.
    assert!(!html.contains("<h2>Schedule</h2>"), "{html}");
    assert!(!html.contains("<h3"), "no per-part heading for a single part: {html}");
    assert!(html.contains("<dd>Saturday, 5 September 2026<br>4:00 PM</dd>"), "{html}");
}

#[sqlx::test]
async fn user_content_cannot_inject_markup(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "organizer@example.com").await;
    let slug = seed_event(
        &app,
        &session,
        json!({
            "title": "<script>alert('xss')</script>",
            "description": "\" onload=\"alert(1)",
            "subEvents": [{
                "name": "<img src=x onerror=alert(1)>",
                "startsAt": "2026-09-05T15:00:00Z",
                "venueName": "</style><script>alert(2)</script>"
            }]
        }),
    )
    .await;

    let (status, html, _) = get_html(&app, &format!("/e/{slug}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!html.contains("<script>alert"), "script tag escaped: {html}");
    assert!(!html.contains("<img src=x"), "img tag escaped: {html}");
    assert!(!html.contains(r#"" onload=""#), "attribute break escaped: {html}");
    assert!(html.contains("&#60;script&#62;"), "escaped form present: {html}");
}

#[sqlx::test]
async fn cover_image_drives_the_preview_image(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "organizer@example.com").await;
    let slug = seed_event(
        &app,
        &session,
        json!({
            "title": "Gala",
            "coverImageUrl": "https://cdn.example.com/gala.jpg",
            "subEvents": [{ "name": "Gala", "startsAt": "2026-09-05T15:00:00Z", "isDefault": true }]
        }),
    )
    .await;

    let (_, html, _) = get_html(&app, &format!("/e/{slug}")).await;
    assert!(html.contains(r#"<meta property="og:image" content="https://cdn.example.com/gala.jpg">"#));
    assert!(html.contains(r#"<img class="hero-img" src="https://cdn.example.com/gala.jpg""#));
    // Dimensions are unknown for a third-party image; better absent than wrong.
    assert!(!html.contains("og:image:width"), "{html}");
}

#[sqlx::test]
async fn cover_image_must_be_an_http_url(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "organizer@example.com").await;

    for bad in ["javascript:alert(1)", "data:text/html,<script>alert(1)</script>", "/etc/passwd"] {
        let (status, body, _) = send(
            &app,
            Method::POST,
            "/api/events",
            Some(json!({
                "title": "Gala",
                "coverImageUrl": bad,
                "subEvents": [{ "name": "Gala", "startsAt": "2026-09-05T15:00:00Z" }]
            })),
            Some(&session),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad} was accepted: {body}");
    }

    // And the same on update.
    let slug = seed_event(
        &app,
        &session,
        json!({ "title": "Gala", "subEvents": [{ "name": "Gala", "startsAt": "2026-09-05T15:00:00Z" }] }),
    )
    .await;
    let (_, event, _) = send(&app, Method::GET, "/api/events", None, Some(&session)).await;
    let id = event[0]["id"].as_str().unwrap();
    let (status, body, _) = send(
        &app,
        Method::PATCH,
        &format!("/api/events/{id}"),
        Some(json!({ "coverImageUrl": "javascript:alert(1)" })),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // The page for that event still renders, with the default preview image.
    let (status, html, _) = get_html(&app, &format!("/e/{slug}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("og-default-v1.png"));
}

#[sqlx::test]
async fn unknown_slug_renders_a_branded_404(pool: PgPool) {
    let app = app(pool.clone());

    let (status, html, headers) = get_html(&app, "/e/no-such-party").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert!(html.contains("This invitation isn&#39;t here"), "{html}");
    assert!(html.contains("106"), "still branded: {html}");
}

#[sqlx::test]
async fn deleting_an_event_takes_its_public_page_down(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "organizer@example.com").await;
    let slug = seed_event(&app, &session, wedding()).await;

    let (_, events, _) = send(&app, Method::GET, "/api/events", None, Some(&session)).await;
    let id = events[0]["id"].as_str().unwrap();
    let (status, _, _) = send(&app, Method::DELETE, &format!("/api/events/{id}"), None, Some(&session)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _, _) = get_html(&app, &format!("/e/{slug}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn static_assets_are_served_immutably(pool: PgPool) {
    let app = app(pool.clone());

    for (path, content_type) in [
        ("/static/og-default-v1.png", "image/png"),
        ("/static/favicon-v1.svg", "image/svg+xml"),
        ("/static/fonts/fraunces-latin-v1.woff2", "font/woff2"),
        ("/static/fonts/fraunces-latin-ext-v1.woff2", "font/woff2"),
        ("/static/fonts/fraunces-vietnamese-v1.woff2", "font/woff2"),
    ] {
        let (status, body, headers) = get_html(&app, path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_eq!(headers[header::CONTENT_TYPE], content_type, "{path}");
        assert_eq!(
            headers[header::CACHE_CONTROL], "public, max-age=31536000, immutable",
            "{path}"
        );
        assert!(!body.is_empty(), "{path} is empty");
    }
}
