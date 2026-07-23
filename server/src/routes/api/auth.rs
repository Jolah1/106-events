use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    auth::{
        CurrentUser, SESSION_COOKIE, User,
        session::{create_session, delete_session},
        tokens::{self, NewToken},
    },
    error::AppError,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/request-link", post(request_link))
        .route("/auth/verify", post(verify))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestLinkBody {
    email: String,
    /// Optional staff passphrase; see `Config::staff_access_code`.
    #[serde(default)]
    staff_code: String,
}

/// Case-insensitive, trimmed. Full RFC-compliant validation is a losing game;
/// the magic-link email itself is the real verification.
fn normalize_email(raw: &str) -> Result<String, AppError> {
    let email = raw.trim().to_lowercase();
    let valid = email.len() <= 254
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
        });
    if !valid {
        return Err(AppError::validation("enter a valid email address"));
    }
    Ok(email)
}

async fn request_link(
    State(state): State<AppState>,
    Json(body): Json<RequestLinkBody>,
) -> Result<impl IntoResponse, AppError> {
    let email = normalize_email(&body.email)?;

    let recent = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM login_tokens
           WHERE identifier = $1 AND created_at > now() - interval '15 minutes'"#,
        email
    )
    .fetch_one(&state.pool)
    .await?;
    if recent >= 5 {
        return Err(AppError::RateLimited);
    }

    // Only invited staff receive a link — but the endpoint has to behave the
    // same for everyone, or the difference (an email that arrives, a 429 that
    // eventually fires) would reveal who is on the team. So a token is minted
    // and rate-limited regardless; it is only emailed, and only returned as a
    // dev link, when the address actually belongs to a member. A token minted
    // for a stranger maps to no user, so `verify` rejects it either way.
    let is_member = sqlx::query_scalar!("SELECT 1 FROM users WHERE email = $1", email)
        .fetch_optional(&state.pool)
        .await?
        .is_some();

    let NewToken { raw, hash } = tokens::generate_token();
    sqlx::query!(
        "INSERT INTO login_tokens (identifier, token_hash, expires_at)
         VALUES ($1, $2, now() + make_interval(mins => $3::int))",
        email,
        hash,
        state.config.login_token_ttl_minutes as i32
    )
    .execute(&state.pool)
    .await?;

    let mut dev_link = None;
    if is_member {
        let link = format!("{}/auth/verify?token={raw}", state.config.app_base_url);
        state
            .mailer
            .send_magic_link(&email, &link)
            .await
            .map_err(AppError::Internal)?;

        // Only when there is no mailer *and* someone explicitly asked for this.
        // Returning a magic link to whoever requested it is account takeover;
        // it exists so the flow is usable without email infrastructure, and it
        // must never switch itself on because a deploy forgot to set SMTP_URL.
        //
        // The staff code is the per-request version of the same switch: a
        // correct passphrase unlocks the link for this one response. Hashes
        // are compared rather than the strings themselves so a timing
        // difference can't leak how much of a guess was right. A wrong code
        // is indistinguishable from no code — the endpoint stays quiet.
        let code_ok = state.config.staff_access_code.as_deref().is_some_and(|expected| {
            tokens::hash_token(expected) == tokens::hash_token(body.staff_code.trim())
        });
        dev_link = (state.mailer.is_dev() && (state.config.allow_dev_login || code_ok))
            .then_some(link);
    }

    Ok(Json(json!({ "sent": true, "devLink": dev_link })))
}

#[derive(Deserialize)]
struct VerifyBody {
    token: String,
}

async fn verify(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<VerifyBody>,
) -> Result<impl IntoResponse, AppError> {
    // Atomic consume: a link can only ever be used once.
    let identifier = sqlx::query_scalar!(
        "UPDATE login_tokens SET consumed_at = now()
         WHERE token_hash = $1 AND consumed_at IS NULL AND expires_at > now()
         RETURNING identifier",
        tokens::hash_token(body.token.trim())
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::Unauthorized)?;

    // Staff are invited, not self-served: a valid magic link only signs in an
    // email that already has a users row. A working inbox is no longer enough
    // to get an account — otherwise anyone could create events for the agency.
    let user = sqlx::query_as!(
        User,
        "SELECT id, email, phone, name, role, created_at FROM users WHERE email = $1",
        identifier
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::Forbidden)?;

    let session_token = create_session(&state.pool, user.id, state.config.session_ttl_days).await?;
    let cookie = Cookie::build((SESSION_COOKIE, session_token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.config.cookie_secure)
        .max_age(time::Duration::days(state.config.session_ttl_days))
        .build();

    Ok((jar.add(cookie), Json(json!({ "user": user }))))
}

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<impl IntoResponse, AppError> {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        delete_session(&state.pool, cookie.value()).await?;
    }
    let removal = Cookie::build((SESSION_COOKIE, "")).path("/").build();
    Ok((jar.remove(removal), StatusCode::NO_CONTENT))
}

async fn me(CurrentUser(user): CurrentUser) -> Json<serde_json::Value> {
    Json(json!({ "user": user }))
}
