//! The per-event vendor sheet: who's supplying what, and where the money is.
//!
//! Deliberately narrow. This is a tracker the agency fills in themselves — no
//! vendor logins, no marketplace, no payment processing. Money recorded here is
//! going *out* to suppliers; Phase 6's ticketing money comes *in*.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    domain::{csv_import, money, phone},
    error::AppError,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/events/{id}/vendors", get(list_vendors).post(create_vendor))
        .route("/vendors/{id}", patch(update_vendor).delete(delete_vendor))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Vendor {
    pub id: Uuid,
    pub event_id: Uuid,
    pub name: String,
    pub category: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub service: String,
    pub cost_kobo: i64,
    pub amount_paid_kobo: i64,
    /// Derived, never stored: "unpaid", "part_paid", "paid" or "overpaid".
    pub paid_status: String,
    /// Also derived. Clamped at zero so one overpaid vendor can't mask
    /// another's debt in a total.
    pub outstanding_kobo: i64,
    pub notes: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Row shape shared by every query in this module.
struct VendorRow {
    id: Uuid,
    event_id: Uuid,
    name: String,
    category: String,
    phone: Option<String>,
    email: Option<String>,
    service: String,
    cost_kobo: i64,
    amount_paid_kobo: i64,
    notes: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<VendorRow> for Vendor {
    fn from(r: VendorRow) -> Self {
        Vendor {
            paid_status: money::paid_status(r.cost_kobo, r.amount_paid_kobo)
                .as_str()
                .to_string(),
            outstanding_kobo: money::outstanding_kobo(r.cost_kobo, r.amount_paid_kobo),
            id: r.id,
            event_id: r.event_id,
            name: r.name,
            category: r.category,
            phone: r.phone,
            email: r.email,
            service: r.service,
            cost_kobo: r.cost_kobo,
            amount_paid_kobo: r.amount_paid_kobo,
            notes: r.notes,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewVendor {
    pub name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub service: String,
    #[serde(default)]
    pub cost_kobo: i64,
    #[serde(default)]
    pub amount_paid_kobo: i64,
    #[serde(default)]
    pub notes: String,
}

/// Absent leaves a field alone; `null` clears it. Same double-option trick the
/// guests and sub-events endpoints use.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorPatch {
    pub name: Option<String>,
    pub category: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub phone: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub email: Option<Option<String>>,
    pub service: Option<String>,
    pub cost_kobo: Option<i64>,
    pub amount_paid_kobo: Option<i64>,
    pub notes: Option<String>,
}

fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

/// ₦10 billion. Far beyond any real line item, but a ceiling stops a slipped
/// digit becoming a total nobody can read.
const MAX_KOBO: i64 = 1_000_000_000_000;

fn validate_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 200 {
        return Err(AppError::validation("a vendor needs a name of 1-200 characters"));
    }
    Ok(name.to_string())
}

fn validate_amount(kobo: i64, label: &str) -> Result<i64, AppError> {
    if kobo < 0 {
        return Err(AppError::validation(format!("{label} can't be negative")));
    }
    if kobo > MAX_KOBO {
        return Err(AppError::validation(format!("{label} is implausibly large")));
    }
    Ok(kobo)
}

/// Same normalization as guests: a vendor's number should be dialable and
/// should match however it was typed elsewhere.
fn validate_phone(raw: &str) -> Result<Option<String>, AppError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    phone::normalize(raw)
        .map(Some)
        .ok_or_else(|| AppError::validation(format!("{raw:?} isn't a phone number we can call")))
}

fn validate_email(raw: &str) -> Result<Option<String>, AppError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    if !csv_import::is_emailish(raw) {
        return Err(AppError::validation(format!("{raw:?} isn't an email address")));
    }
    Ok(Some(raw.to_lowercase()))
}

async fn event_exists(pool: &PgPool, event_id: Uuid) -> Result<(), AppError> {
    sqlx::query_scalar!(r#"SELECT 1 AS "one!" FROM events WHERE id = $1"#, event_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(())
}

async fn list_vendors(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(event_id): Path<Uuid>,
) -> Result<Json<Vec<Vendor>>, AppError> {
    event_exists(&state.pool, event_id).await?;
    let rows = sqlx::query_as!(
        VendorRow,
        "SELECT id, event_id, name, category, phone, email, service,
                cost_kobo, amount_paid_kobo, notes, created_at, updated_at
         FROM vendors WHERE event_id = $1 ORDER BY name",
        event_id
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows.into_iter().map(Vendor::from).collect()))
}

async fn create_vendor(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(event_id): Path<Uuid>,
    Json(body): Json<NewVendor>,
) -> Result<impl IntoResponse, AppError> {
    event_exists(&state.pool, event_id).await?;

    let name = validate_name(&body.name)?;
    let cost = validate_amount(body.cost_kobo, "cost")?;
    let paid = validate_amount(body.amount_paid_kobo, "amount paid")?;
    let phone = validate_phone(&body.phone)?;
    let email = validate_email(&body.email)?;

    let row = sqlx::query_as!(
        VendorRow,
        "INSERT INTO vendors
             (event_id, name, category, phone, email, service, cost_kobo, amount_paid_kobo, notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id, event_id, name, category, phone, email, service,
                   cost_kobo, amount_paid_kobo, notes, created_at, updated_at",
        event_id,
        name,
        body.category.trim(),
        phone,
        email,
        body.service.trim(),
        cost,
        paid,
        body.notes.trim()
    )
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(Vendor::from(row))))
}

async fn update_vendor(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(patch): Json<VendorPatch>,
) -> Result<Json<Vendor>, AppError> {
    let name = patch.name.as_deref().map(validate_name).transpose()?;
    let cost = patch
        .cost_kobo
        .map(|k| validate_amount(k, "cost"))
        .transpose()?;
    let paid = patch
        .amount_paid_kobo
        .map(|k| validate_amount(k, "amount paid"))
        .transpose()?;
    // Inner None means "clear it"; the outer None means "leave it".
    let phone = match &patch.phone {
        Some(Some(raw)) => Some(validate_phone(raw)?),
        Some(None) => Some(None),
        None => None,
    };
    let email = match &patch.email {
        Some(Some(raw)) => Some(validate_email(raw)?),
        Some(None) => Some(None),
        None => None,
    };

    let row = sqlx::query_as!(
        VendorRow,
        r#"
        UPDATE vendors SET
            name             = COALESCE($2, name),
            category         = COALESCE($3, category),
            phone            = CASE WHEN $4 THEN $5 ELSE phone END,
            email            = CASE WHEN $6 THEN $7 ELSE email END,
            service          = COALESCE($8, service),
            cost_kobo        = COALESCE($9, cost_kobo),
            amount_paid_kobo = COALESCE($10, amount_paid_kobo),
            notes            = COALESCE($11, notes),
            updated_at       = now()
        WHERE id = $1
        RETURNING id, event_id, name, category, phone, email, service,
                  cost_kobo, amount_paid_kobo, notes, created_at, updated_at
        "#,
        id,
        name,
        patch.category.as_deref().map(str::trim),
        phone.is_some(),
        phone.flatten(),
        email.is_some(),
        email.flatten(),
        patch.service.as_deref().map(str::trim),
        cost,
        paid,
        patch.notes.as_deref().map(str::trim),
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(Vendor::from(row)))
}

async fn delete_vendor(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let deleted = sqlx::query!("DELETE FROM vendors WHERE id = $1", id)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
