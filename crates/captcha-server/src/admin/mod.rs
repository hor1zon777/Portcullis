pub mod auth;
pub mod handlers;
pub mod request_log;

use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::state::AppState;

pub fn admin_router(state: AppState, token: String) -> Router {
    Router::new()
        .route("/admin", get(handlers::dashboard_page))
        .route("/admin/api/stats", get(handlers::stats))
        .route("/admin/api/sites", get(handlers::list_sites))
        .route("/admin/api/sites", post(handlers::create_site))
        .route("/admin/api/sites/:key", put(handlers::update_site))
        .route("/admin/api/sites/:key", delete(handlers::delete_site))
        .route("/admin/api/logs", get(handlers::logs))
        .route("/admin/api/risk/ips", get(handlers::risk_ips))
        .route("/admin/api/risk/block", post(handlers::block_ip))
        .route("/admin/api/risk/block", delete(handlers::unblock_ip))
        .with_state(state)
        .layer(axum::middleware::from_fn(move |req: Request<axum::body::Body>, next: Next| {
            let expected = token.clone();
            async move {
                let query = req.uri().query().unwrap_or("");
                let from_query = query
                    .split('&')
                    .find_map(|p| p.strip_prefix("token="))
                    .map(|s| s.to_string());
                let from_header = req
                    .headers()
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.strip_prefix("Bearer "))
                    .map(|s| s.to_string());

                let provided = from_query.or(from_header);
                match provided {
                    Some(t) if t == expected => next.run(req).await,
                    _ => (
                        StatusCode::UNAUTHORIZED,
                        "未授权，请提供正确的 admin token",
                    )
                        .into_response(),
                }
            }
        }))
}
