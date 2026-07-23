//! Public, unauthenticated, server-rendered pages.
//!
//! These are the pages guests actually see, usually opened from a WhatsApp
//! message on a mid-range Android phone over patchy data. They are plain HTML
//! with inlined CSS and no JavaScript: one request paints the page.

use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::{
        StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE},
    },
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use uuid::Uuid;

use crate::{
    domain::{
        code,
        datetime::{date_summary, day_label, time_range},
    },
    error::AppError,
    routes::api::{
        access_requests, check_in,
        rsvp_store::{self, PartResponse},
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(landing_page))
        .route("/request-access", post(request_access))
        .route("/e/{slug}", get(event_page))
        .route("/r/{token}", get(rsvp_page).post(rsvp_submit))
        .route("/q/{code}", get(qr_image))
        .route("/static/favicon-v1.svg", get(favicon))
        .route("/static/og-default-v1.png", get(og_default))
        .route("/static/img/logo-v1.webp", get(img_logo))
        .route("/static/img/hero-v2.webp", get(img_hero))
        .route("/static/img/weddings-v1.webp", get(img_weddings))
        .route("/static/img/corporate-v1.webp", get(img_corporate))
        .route("/static/img/social-v1.webp", get(img_social))
        .route("/static/img/founder-v1.webp", get(img_founder))
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

// ---------------------------------------------------------------------------
// Landing page

#[derive(Template)]
#[template(path = "public/landing.html")]
struct LandingPage {
    meta: Meta,
    /// Set after a successful request, so the form can say so without needing
    /// any state beyond the URL.
    requested: bool,
    /// Echoed back into the form when a submission is rejected, so nobody has
    /// to retype what they already wrote.
    form: RequestForm,
    error: Option<String>,
}

#[derive(Default, serde::Deserialize)]
pub struct RequestForm {
    #[serde(default)]
    name: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    phone: String,
    #[serde(default)]
    about: String,
    #[serde(default)]
    budget: String,
    /// A field no human sees and no human fills. Bots fill every input they
    /// find, so anything here means the submission wasn't typed by a person.
    #[serde(default)]
    website: String,
}

#[derive(serde::Deserialize)]
struct LandingQuery {
    #[serde(default)]
    requested: bool,
}

fn landing_meta(state: &AppState) -> Meta {
    let base = state.config.public_base_url.clone();
    Meta {
        page_title: "106 Events — guest lists, RSVPs and the door, for any event".into(),
        og_title: "106 Events".into(),
        og_description: "Guest lists, RSVPs, reminders and QR check-in at the door — \
                         for weddings, corporate events and every celebration in between."
            .into(),
        og_image: format!("{base}/static/og-default-v1.png"),
        og_image_alt: "106 Events".into(),
        og_image_dims: Some(Dims { width: 1200, height: 630 }),
        canonical_url: base.clone(),
        home_url: base,
    }
}

async fn landing_page(
    State(state): State<AppState>,
    Query(query): Query<LandingQuery>,
) -> Response {
    let page = LandingPage {
        meta: landing_meta(&state),
        requested: query.requested,
        form: RequestForm::default(),
        error: None,
    };
    // Public and identical for everyone, but short-lived: the copy changes as
    // the product does, and nobody should be reading last month's page.
    render(page, StatusCode::OK, "public, max-age=300")
}

/// Takes a request for an account from the landing page.
///
/// A plain form post with no JavaScript, like every other public page here: one
/// request in, one redirect back. Failure re-renders the page with what they
/// typed still in the fields rather than dumping them on an error screen.
async fn request_access(
    State(state): State<AppState>,
    Form(form): Form<RequestForm>,
) -> Response {
    // A filled honeypot is a bot. Answer exactly as though it worked, so it
    // learns nothing, and write nothing down.
    if !form.website.is_empty() {
        return Redirect::to(&format!("{}/?requested=true", state.config.public_base_url))
            .into_response();
    }

    match access_requests::record(
        &state.pool,
        &form.name,
        &form.email,
        &form.phone,
        &form.about,
        &form.budget,
    )
    .await
    {
        Ok(()) => Redirect::to(&format!("{}/?requested=true", state.config.public_base_url))
            .into_response(),
        Err(AppError::Validation(message)) => render(
            LandingPage {
                meta: landing_meta(&state),
                requested: false,
                form,
                error: Some(message),
            },
            StatusCode::BAD_REQUEST,
            "no-store",
        ),
        Err(other) => other.into_response(),
    }
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

// ---------------------------------------------------------------------------
// Public RSVP link
//
// Reached from a WhatsApp/SMS message: /r/{token}, where the token is the
// guest's unguessable rsvp_token. One request, no login, no JavaScript — the
// form posts back and we redirect (POST/redirect/GET) so a refresh can't
// double-submit.
// ---------------------------------------------------------------------------

struct RsvpPartView {
    id: Uuid,
    name: String,
    day: String,
    time: String,
    venue: String,
    /// Whether the guest has this part marked as attending (a confirmed RSVP);
    /// drives the checkbox's initial state.
    attending: bool,
    party_size: i32,
}

#[derive(Template)]
#[template(path = "public/rsvp.html")]
struct RsvpPage {
    meta: Meta,
    guest_name: String,
    event_title: String,
    date_summary: String,
    token: Uuid,
    parts: Vec<RsvpPartView>,
    /// The most anyone may bring: 1 + the guest's plus-ones. Bounds the party
    /// selector.
    max_party: i32,
    /// True just after a submission, to show a "saved" note.
    saved: bool,
    /// A single default part hides the schedule layer, matching the event page.
    single: bool,
    /// One pass per person the guest confirmed for. Empty until they confirm.
    passes: Vec<PassView>,
}

/// A pass is what gets shown at the door: a square to scan and, underneath it,
/// the same code in characters for when the scanner won't cooperate.
struct PassView {
    label: String,
    /// The code spaced into two groups, which is how it gets read aloud and
    /// typed back in. `code::normalize` strips the space again at the door.
    spoken: String,
    qr_url: String,
}

/// The guest's own codes, lowest head first, capped at what they confirmed for.
///
/// Extras added at the door are excluded: those belong to whoever staff let in,
/// and putting one on the inviting guest's phone would be a second pass for a
/// person who already walked through.
async fn load_passes(
    state: &AppState,
    guest_id: Uuid,
    guest_name: &str,
    heads: i32,
) -> Result<Vec<PassView>, AppError> {
    let rows = sqlx::query!(
        "SELECT head_index, code FROM attendees
         WHERE guest_id = $1 AND NOT is_extra
         ORDER BY head_index
         LIMIT $2",
        guest_id,
        heads as i64
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| PassView {
            label: if r.head_index == 0 {
                guest_name.to_string()
            } else {
                format!("{guest_name} +{}", r.head_index)
            },
            spoken: format!("{} {}", &r.code[..4], &r.code[4..]),
            qr_url: format!("{}/q/{}", state.config.public_base_url, r.code),
        })
        .collect())
}

async fn load_rsvp_page(
    state: &AppState,
    token: Uuid,
    saved: bool,
) -> Result<Option<RsvpPage>, AppError> {
    let Some(guest) = rsvp_store::guest_by_token(&state.pool, token).await? else {
        return Ok(None);
    };

    let event = sqlx::query!(
        "SELECT title, timezone FROM events WHERE id = $1",
        guest.event_id
    )
    .fetch_one(&state.pool)
    .await?;
    let tz: Tz = event.timezone.parse().unwrap_or(chrono_tz::Africa::Lagos);

    let rows = sqlx::query!(
        "SELECT se.id, se.name, se.starts_at, se.ends_at, se.venue_name, se.is_default,
                gi.rsvp_status, gi.party_size
         FROM guest_invites gi
         JOIN sub_events se ON se.id = gi.sub_event_id
         WHERE gi.guest_id = $1
         ORDER BY se.position, se.starts_at",
        guest.id
    )
    .fetch_all(&state.pool)
    .await?;

    let starts: Vec<DateTime<Utc>> = rows.iter().map(|r| r.starts_at).collect();
    let date_line = match (starts.iter().min(), starts.iter().max()) {
        (Some(first), Some(last)) => date_summary(*first, *last, tz),
        _ => String::new(),
    };
    let single = rows.len() == 1 && rows[0].is_default;
    let max_party = 1 + guest.plus_ones;

    // Show only as many passes as the largest party they actually confirmed
    // for. Handing someone four squares when they said they're coming alone
    // invites exactly the confusion the door then has to sort out.
    let confirmed_heads = rows
        .iter()
        .filter(|r| r.rsvp_status == "confirmed")
        .map(|r| r.party_size)
        .max()
        .unwrap_or(0);
    let passes = if confirmed_heads > 0 {
        load_passes(state, guest.id, &guest.name, confirmed_heads).await?
    } else {
        Vec::new()
    };

    let parts = rows
        .into_iter()
        .map(|r| RsvpPartView {
            id: r.id,
            name: r.name,
            day: day_label(r.starts_at, tz),
            time: time_range(r.starts_at, r.ends_at, tz),
            venue: r.venue_name,
            attending: r.rsvp_status == "confirmed",
            // Default the selector to a full turnout so a first-time confirm
            // doesn't start at a lonely 1.
            party_size: if r.party_size > 0 { r.party_size } else { max_party },
        })
        .collect();

    Ok(Some(RsvpPage {
        meta: Meta {
            page_title: format!("RSVP · {} · 106 Events", event.title),
            og_title: format!("RSVP · {}", event.title),
            og_description: "Let them know if you'll be there.".into(),
            og_image: format!("{}/static/og-default-v1.png", state.config.public_base_url),
            og_image_alt: "106 Events".into(),
            og_image_dims: Some(Dims { width: 1200, height: 630 }),
            canonical_url: format!("{}/r/{}", state.config.public_base_url, token),
            home_url: state.config.public_base_url.clone(),
        },
        guest_name: guest.name,
        event_title: event.title,
        date_summary: date_line,
        token,
        parts,
        max_party,
        saved,
        single,
        passes,
    }))
}

#[derive(serde::Deserialize)]
struct RsvpQuery {
    #[serde(default)]
    saved: bool,
}

async fn rsvp_page(
    State(state): State<AppState>,
    Path(token): Path<Uuid>,
    Query(query): Query<RsvpQuery>,
) -> Result<Response, AppError> {
    match load_rsvp_page(&state, token, query.saved).await? {
        // The RSVP link must never be cached: it shows a guest's own answers.
        Some(page) => Ok(render(page, StatusCode::OK, "no-store")),
        None => Ok(not_found_page(&state)),
    }
}

async fn rsvp_submit(
    State(state): State<AppState>,
    Path(token): Path<Uuid>,
    Form(fields): Form<Vec<(String, String)>>,
) -> Result<Response, AppError> {
    let Some(guest) = rsvp_store::guest_by_token(&state.pool, token).await? else {
        return Ok(not_found_page(&state));
    };

    // The checkboxes: every `attending=<uuid>` names a part the guest is
    // coming to. Everything they're invited to but didn't tick is a decline.
    let attending: std::collections::HashSet<Uuid> = fields
        .iter()
        .filter(|(k, _)| k == "attending")
        .filter_map(|(_, v)| v.parse().ok())
        .collect();

    let invited: Vec<Uuid> = sqlx::query_scalar!(
        "SELECT sub_event_id FROM guest_invites WHERE guest_id = $1",
        guest.id
    )
    .fetch_all(&state.pool)
    .await?;

    let responses: Vec<PartResponse> = invited
        .into_iter()
        .map(|sub_event_id| {
            let party_size = fields
                .iter()
                .find(|(k, _)| *k == format!("party_{sub_event_id}"))
                .and_then(|(_, v)| v.parse().ok())
                .unwrap_or(1);
            PartResponse {
                sub_event_id,
                attending: attending.contains(&sub_event_id),
                party_size,
            }
        })
        .collect();

    rsvp_store::record_link_response(&state.pool, &guest, &responses).await?;

    // Issue their passes now, so the page they're about to land on can show
    // them. Idempotent, and cheap enough to run on every submission.
    check_in::ensure_heads_for_guest(
        &state.pool,
        guest.id,
        guest.event_id,
        1 + guest.plus_ones,
    )
    .await?;

    // POST/redirect/GET: land back on the page (now showing "saved") so a
    // refresh re-fetches rather than re-submits.
    Ok(Redirect::to(&format!("{}/r/{}?saved=true", state.config.public_base_url, token)).into_response())
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

/// A head's QR code, as an image.
///
/// Unauthenticated on purpose: the code *is* the credential, so anyone who can
/// request this URL already holds everything it encodes. Being a plain image URL
/// is what lets the same square appear on the RSVP page, in a WhatsApp message,
/// and on a printed invitation without three implementations.
///
/// It renders whatever plausible code is asked for without checking the
/// database, so it can't be used to enumerate who is on a guest list.
async fn qr_image(Path(raw): Path<String>) -> Response {
    let code = code::normalize(&raw);
    if !code::is_plausible(&code) {
        return StatusCode::NOT_FOUND.into_response();
    }
    (
        [
            (CONTENT_TYPE, "image/svg+xml"),
            // Codes are issued once and never rotated, so the square for a
            // given code is permanent.
            (CACHE_CONTROL, IMMUTABLE),
        ],
        code::qr_svg(&code),
    )
        .into_response()
}

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

// Landing photography. Licensed for this use: the hero and category shots are
// Pexels (free commercial use, no attribution required); the founder portrait
// was supplied by the company. Baked in like the fonts — same reasoning.
async fn img_logo() -> Response {
    asset(include_bytes!("../../static/img/logo.webp"), "image/webp")
}

async fn img_hero() -> Response {
    asset(include_bytes!("../../static/img/hero.webp"), "image/webp")
}

async fn img_weddings() -> Response {
    asset(include_bytes!("../../static/img/weddings.webp"), "image/webp")
}

async fn img_corporate() -> Response {
    asset(include_bytes!("../../static/img/corporate.webp"), "image/webp")
}

async fn img_social() -> Response {
    asset(include_bytes!("../../static/img/social.webp"), "image/webp")
}

async fn img_founder() -> Response {
    asset(include_bytes!("../../static/img/founder.webp"), "image/webp")
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
