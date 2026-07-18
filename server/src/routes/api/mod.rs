pub mod access_requests;
pub mod auth;
pub mod check_in;
pub mod config;
pub mod events;
pub mod guests;
pub mod reminders;
pub mod rsvp_store;
pub mod team;
pub mod vendors;
pub mod webhooks;

use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(access_requests::router())
        .merge(auth::router())
        .merge(check_in::router())
        .merge(config::router())
        .merge(events::router())
        .merge(guests::router())
        .merge(reminders::router())
        .merge(team::router())
        .merge(vendors::router())
        .merge(webhooks::router())
}
