pub mod config;
pub mod error;
pub mod routes;
pub mod state;
pub mod static_assets;
pub mod store;
pub mod token;

use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};

use crate::state::AppState;

pub fn build_router(app_state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/v1/challenge", post(routes::challenge::create))
        .route("/api/v1/verify", post(routes::verify::verify))
        .route("/api/v1/siteverify", post(routes::siteverify::site_verify))
        .route("/sdk/*file", get(static_assets::serve_sdk))
        .route("/healthz", get(|| async { "ok" }))
        .layer(cors)
        .with_state(app_state)
}
