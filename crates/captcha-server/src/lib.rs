pub mod config;
pub mod error;
pub mod metrics;
pub mod rate_limit;
pub mod risk;
pub mod routes;
pub mod state;
pub mod static_assets;
pub mod store;
pub mod token;

use axum::http::HeaderValue;
use axum::middleware;
use axum::routing::{get, post};
use axum::Router;
use metrics_exporter_prometheus::PrometheusHandle;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::rate_limit::{rate_limit_middleware, IpRateLimiter};
use crate::state::AppState;

const BODY_LIMIT: usize = 256 * 1024; // 256 KiB（batch verify 可能较大）

pub fn build_router(
    app_state: AppState,
    limiter: Option<IpRateLimiter>,
    prom_handle: Option<PrometheusHandle>,
) -> Router {
    let config = app_state.config.load();
    let allowed_origins: Vec<HeaderValue> = config
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
        .route("/api/v1/verify/batch", post(routes::verify::verify_batch))
        .route("/api/v1/siteverify", post(routes::siteverify::site_verify))
        .route("/sdk/*file", get(static_assets::serve_sdk))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(app_state);

    if let Some(handle) = prom_handle {
        router = router.merge(
            Router::new()
                .route("/metrics", get(metrics::metrics_handler))
                .with_state(handle),
        );
    }

    if let Some(lim) = limiter {
        router = router.route_layer(middleware::from_fn_with_state(lim, rate_limit_middleware));
    }

    router
        .layer(cors)
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT))
        .layer(CompressionLayer::new())
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
