pub mod config;
pub mod error;
pub mod rate_limit;
pub mod routes;
pub mod state;
pub mod static_assets;
pub mod store;
pub mod token;

use axum::http::HeaderValue;
use axum::middleware;
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::rate_limit::{rate_limit_middleware, IpRateLimiter};
use crate::state::AppState;

const BODY_LIMIT: usize = 64 * 1024;

/// 构建完整路由。可选注入 IP 限流器（测试时传 None 跳过限流）。
pub fn build_router(app_state: AppState, limiter: Option<IpRateLimiter>) -> Router {
    let allowed_origins: Vec<HeaderValue> = app_state
        .config
        .sites
        .values()
        .flat_map(|s| &s.origins)
        .filter_map(|o| o.parse::<HeaderValue>().ok())
        .collect();

    let cors = if allowed_origins.is_empty() {
        CorsLayer::permissive()
    } else {
        CorsLayer::new()
            .allow_origin(allowed_origins)
            .allow_methods(tower_http::cors::Any)
            .allow_headers(tower_http::cors::Any)
    };

    let mut router = Router::new()
        .route("/api/v1/challenge", post(routes::challenge::create))
        .route("/api/v1/verify", post(routes::verify::verify))
        .route("/api/v1/siteverify", post(routes::siteverify::site_verify))
        .route("/sdk/*file", get(static_assets::serve_sdk))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(app_state);

    // IP 限流层（测试时不注入，避免无 ConnectInfo 的环境问题）
    if let Some(lim) = limiter {
        router = router
            .route_layer(middleware::from_fn_with_state(lim, rate_limit_middleware));
    }

    router
        .layer(cors)
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
}
