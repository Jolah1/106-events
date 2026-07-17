use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/config", get(public_config))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicConfig {
    /// Origin that serves public event pages. The dashboard can't infer this:
    /// in development it runs on a different port to this server.
    public_base_url: String,
}

async fn public_config(State(state): State<AppState>) -> Json<PublicConfig> {
    Json(PublicConfig {
        public_base_url: state.config.public_base_url.clone(),
    })
}
