pub mod auth;
pub mod handlers;
pub mod request_log;

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
        .layer(axum::middleware::from_fn(
            move |query, headers, req: axum::http::Request<axum::body::Body>, next| {
                let t = token.clone();
                async move {
                    let mut req = req;
                    req.extensions_mut().insert(t);
                    auth::auth_middleware(query, headers, req, next).await
                }
            },
        ))
}
