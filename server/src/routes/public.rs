//! Public, unauthenticated, server-rendered pages.
//!
//! These are the pages guests actually see, usually opened from a WhatsApp
//! message on a mid-range Android phone over patchy data. They are plain HTML
//! with inlined CSS and no JavaScript: one request paints the page.

use askama::Template;
use axum::{
    Router,
    extract::{Path, State},
    http::{
        StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE},
    },
    response::{Html, IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;

use crate::{
    domain::datetime::{date_summary, day_label, time_range},
    error::AppError,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/e/{slug}", get(event_page))
        .route("/static/favicon-v1.svg", get(favicon))
        .route("/static/og-default-v1.png", get(og_default))
        .route("/static/fonts/fraunces-latin-v1.woff2", get(font_latin))
        .route("/static/fonts/fraunces-latin-ext-v1.woff2", get(font_latin_ext))
        .route("/static/fonts/fraunces-vietnamese-v1.woff2", get(font_vietnamese))
}

/// Everything the shared `<head>` needs.
struct Meta {
    page_title: String,
    og_title: String,
    og_description: String,
    og_image: String,
    og_image_alt: String,
    /// Known only for our own fallback image; a custom cover has unknown size.
    og_image_dims: Option<Dims>,
    canonical_url: String,
    home_url: String,
}

struct Dims {
    width: u32,
    height: u32,
}

struct PartView {
    name: String,
    day: String,
    time: String,
    venue_name: String,
    venue_address: String,
    has_venue: bool,
    /// Free-text location for a maps search link; None when there's no venue.
    map_query: Option<String>,
}

#[derive(Template)]
#[template(path = "public/event.html")]
struct EventPage {
    meta: Meta,
    title: String,
    description: String,
    date_summary: String,
    cover_image_url: Option<String>,
    parts: Vec<PartView>,
    /// A single default part means the event has no meaningful schedule layer:
    /// show one "when & where" block instead of a list of named parts.
    single: bool,
}

#[derive(Template)]
#[template(path = "public/not_found.html")]
struct NotFoundPage {
    meta: Meta,
}

/// Only ever emit `http(s)` URLs into `src`/`og:image`. Cover URLs are
/// validated on write too; this is the belt to that pair of braces, and it also
/// covers rows written before that validation existed.
fn safe_http_url(raw: &str) -> Option<String> {
    let url = raw.trim();
    (url.starts_with("https://") || url.starts_with("http://")).then(|| url.to_string())
}

/// First non-empty line, capped — link previews truncate anyway, and a wall of
/// text in a WhatsApp card looks broken.
fn preview_text(description: &str, fallback: &str) -> String {
    let line = description.trim();
    if line.is_empty() {
        return fallback.to_string();
    }
    const LIMIT: usize = 200;
    if line.chars().count() <= LIMIT {
        return line.to_string();
    }
    let truncated: String = line.chars().take(LIMIT).collect();
    // Prefer cutting at the last word boundary so we don't sever a word.
    let cut = truncated.rfind(' ').unwrap_or(truncated.len());
    format!("{}…", truncated[..cut].trim_end_matches([',', '.', ';', ':']))
}

async fn event_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Response, AppError> {
    let Some(event) = sqlx::query!(
        "SELECT id, title, description, cover_image_url, timezone FROM events WHERE slug = $1",
        slug
    )
    .fetch_optional(&state.pool)
    .await?
    else {
        return Ok(not_found_page(&state));
    };

    let parts = sqlx::query!(
        "SELECT name, description, starts_at, ends_at, venue_name, venue_address, is_default
         FROM sub_events WHERE event_id = $1 ORDER BY position, starts_at",
        event.id
    )
    .fetch_all(&state.pool)
    .await?;

    // Timezones are validated on write; fall back rather than 500 a guest page.
    let tz: Tz = event.timezone.parse().unwrap_or(chrono_tz::Africa::Lagos);

    let starts: Vec<DateTime<Utc>> = parts.iter().map(|p| p.starts_at).collect();
    let date_line = match (starts.iter().min(), starts.iter().max()) {
        (Some(first), Some(last)) => date_summary(*first, *last, tz),
        // Guarded by "every event has at least one sub-event", but a page that
        // renders without a date beats a 500 if that invariant ever slips.
        _ => String::new(),
    };

    let single = parts.len() == 1 && parts[0].is_default;
    let canonical_url = format!("{}/e/{}", state.config.public_base_url, slug);
    let cover_image_url = event.cover_image_url.as_deref().and_then(safe_http_url);

    let (og_image, og_image_alt, og_image_dims) = match &cover_image_url {
        Some(cover) => (cover.clone(), event.title.clone(), None),
        None => (
            format!("{}/static/og-default-v1.png", state.config.public_base_url),
            "106 Events".to_string(),
            Some(Dims { width: 1200, height: 630 }),
        ),
    };

    let page = EventPage {
        meta: Meta {
            page_title: format!("{} · 106 Events", event.title),
            og_title: event.title.clone(),
            og_description: preview_text(&event.description, &date_line),
            og_image,
            og_image_alt,
            og_image_dims,
            canonical_url,
            home_url: state.config.public_base_url.clone(),
        },
        title: event.title,
        description: event.description.trim().to_string(),
        date_summary: date_line,
        cover_image_url,
        parts: parts
            .into_iter()
            .map(|p| {
                let has_venue = !p.venue_name.is_empty() || !p.venue_address.is_empty();
                let map_query = has_venue.then(|| {
                    [p.venue_name.as_str(), p.venue_address.as_str()]
                        .iter()
                        .filter(|s| !s.is_empty())
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                PartView {
                    name: p.name,
                    day: day_label(p.starts_at, tz),
                    time: time_range(p.starts_at, p.ends_at, tz),
                    venue_name: p.venue_name,
                    venue_address: p.venue_address,
                    has_venue,
                    map_query,
                }
            })
            .collect(),
        single,
    };

    Ok(render(page, StatusCode::OK, "public, max-age=60"))
}

fn not_found_page(state: &AppState) -> Response {
    let page = NotFoundPage {
        meta: Meta {
            page_title: "Not found · 106 Events".into(),
            og_title: "106 Events".into(),
            og_description: "This invitation isn't here.".into(),
            og_image: format!("{}/static/og-default-v1.png", state.config.public_base_url),
            og_image_alt: "106 Events".into(),
            og_image_dims: Some(Dims { width: 1200, height: 630 }),
            canonical_url: state.config.public_base_url.clone(),
            home_url: state.config.public_base_url.clone(),
        },
    };
    render(page, StatusCode::NOT_FOUND, "no-store")
}

fn render(page: impl Template, status: StatusCode, cache_control: &str) -> Response {
    match page.render() {
        Ok(html) => (status, [(CACHE_CONTROL, cache_control)], Html(html)).into_response(),
        Err(err) => {
            // Rendering can only fail on a formatting error, i.e. a bug here.
            AppError::Internal(anyhow::Error::new(err).context("rendering page")).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Static assets
//
// Embedded in the binary: the deploy artifact stays a single file, and a
// missing asset becomes a compile error instead of a 404 in production.
// Filenames carry a version suffix so they can be cached forever; bump the
// suffix (here and in the templates) when the bytes change.
// ---------------------------------------------------------------------------

const IMMUTABLE: &str = "public, max-age=31536000, immutable";

fn asset(bytes: &'static [u8], content_type: &'static str) -> Response {
    (
        [(CONTENT_TYPE, content_type), (CACHE_CONTROL, IMMUTABLE)],
        bytes,
    )
        .into_response()
}

async fn favicon() -> Response {
    asset(include_bytes!("../../static/favicon.svg"), "image/svg+xml")
}

async fn og_default() -> Response {
    asset(include_bytes!("../../static/og-default.png"), "image/png")
}

async fn font_latin() -> Response {
    asset(
        include_bytes!("../../static/fonts/fraunces-latin-wght-normal.woff2"),
        "font/woff2",
    )
}

async fn font_latin_ext() -> Response {
    asset(
        include_bytes!("../../static/fonts/fraunces-latin-ext-wght-normal.woff2"),
        "font/woff2",
    )
}

async fn font_vietnamese() -> Response {
    asset(
        include_bytes!("../../static/fonts/fraunces-vietnamese-wght-normal.woff2"),
        "font/woff2",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_cover_urls() {
        assert_eq!(safe_http_url("https://cdn.example.com/a.jpg").as_deref(), Some("https://cdn.example.com/a.jpg"));
        assert_eq!(safe_http_url("http://cdn.example.com/a.jpg").as_deref(), Some("http://cdn.example.com/a.jpg"));
        assert!(safe_http_url("javascript:alert(1)").is_none());
        assert!(safe_http_url("data:text/html,<script>").is_none());
        assert!(safe_http_url("/relative.jpg").is_none());
        assert!(safe_http_url("").is_none());
    }

    #[test]
    fn preview_text_falls_back_and_truncates() {
        assert_eq!(preview_text("   ", "21 Nov 2026"), "21 Nov 2026");
        assert_eq!(preview_text("Come celebrate", "x"), "Come celebrate");

        let long = "word ".repeat(80);
        let preview = preview_text(&long, "x");
        assert!(preview.chars().count() <= 201, "{preview}");
        assert!(preview.ends_with('…'));
        assert!(!preview.contains("wor…"), "cuts on a word boundary: {preview}");
    }

    #[test]
    fn preview_text_handles_multibyte_at_the_limit() {
        // Truncation must count characters, not bytes, or this would panic.
        let text = "é".repeat(400);
        let preview = preview_text(&text, "x");
        assert!(preview.ends_with('…'));
    }
}
