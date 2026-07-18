pub mod auth;
pub mod config;
pub mod events;
pub mod guests;
pub mod reminders;
pub mod rsvp_store;
pub mod team;
pub mod webhooks;

use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(auth::router())
        .merge(config::router())
        .merge(events::router())
        .merge(guests::router())
        .merge(reminders::router())
        .merge(team::router())
        .merge(webhooks::router())
}
