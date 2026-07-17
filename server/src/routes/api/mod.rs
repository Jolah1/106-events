pub mod auth;
pub mod config;
pub mod events;

use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(auth::router())
        .merge(config::router())
        .merge(events::router())
}
