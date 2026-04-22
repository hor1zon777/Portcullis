pub mod auth;
pub mod handlers;
pub mod request_log;

use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::state::AppState;

pub fn admin_router(state: AppState, token: String) -> Router {
    let admin_api = Router::new()
        .route("/api/stats", get(handlers::stats))
        .route("/api/sites", get(handlers::list_sites))
        .route("/api/sites", post(handlers::create_site))
        .route("/api/sites/:key", put(handlers::update_site))
        .route("/api/sites/:key", delete(handlers::delete_site))
        .route("/api/logs", get(handlers::logs))
        .route("/api/risk/ips", get(handlers::risk_ips))
        .route("/api/risk/block", post(handlers::block_ip))
        .route("/api/risk/block", delete(handlers::unblock_ip))
        .with_state(state.clone())
        .layer(axum::middleware::from_fn(move |query, headers, req: axum::http::Request<axum::body::Body>, next| {
            let t = token.clone();
            async move {
                // 注入 token 到 extensions
                let mut req = req;
                req.extensions_mut().insert(t);
                auth::auth_middleware(query, headers, req, next).await
            }
        }));

    Router::new()
        .route("/admin", get(handlers::dashboard_page))
        .nest("/admin", admin_api)
}
