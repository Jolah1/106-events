pub mod api;
pub mod public;

use axum::{Router, routing::get};
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let mut router = Router::new()
        .nest("/api", api::router())
        .merge(public::router())
        .route("/healthz", get(|| async { "ok" }));

    // Production single-binary deploy: serve the built dashboard SPA.
    // In development the dashboard runs under Vite and proxies /api here.
    if let Some(dist) = &state.config.dashboard_dist {
        let index = ServeFile::new(dist.join("index.html"));
        router = router.fallback_service(ServeDir::new(dist).fallback(index));
    }

    router.layer(TraceLayer::new_for_http()).with_state(state)
}
