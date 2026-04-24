pub mod audit;
pub mod auth;
pub mod handlers;
pub mod request_log;
pub mod webhook;

use axum::response::Redirect;
use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::state::AppState;

pub fn admin_router(state: AppState, _token: String) -> Router {
    let api = Router::new()
        .route("/admin/api/stats", get(handlers::stats))
        .route("/admin/api/sites", get(handlers::list_sites))
        .route("/admin/api/sites", post(handlers::create_site))
        .route("/admin/api/sites/:key", put(handlers::update_site))
        .route("/admin/api/sites/:key", delete(handlers::delete_site))
        .route("/admin/api/logs", get(handlers::logs))
        .route("/admin/api/risk/ips", get(handlers::risk_ips))
        .route("/admin/api/risk/block", post(handlers::block_ip))
        .route("/admin/api/risk/block", delete(handlers::unblock_ip))
        .route("/admin/api/manifest-pubkey", get(handlers::manifest_pubkey))
        .route(
            "/admin/api/manifest-pubkey/generate",
            post(handlers::generate_manifest_key),
        )
        .route(
            "/admin/api/manifest-pubkey",
            delete(handlers::revoke_manifest_key),
        )
        .route("/admin/api/audit", get(handlers::audit_list))
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state,
            auth::auth_middleware_with_state,
        ));

    // /admin 页面由 admin-ui 容器（Nginx）提供；
    // 单二进制部署时重定向到提示页
    let fallback = Router::new().route("/admin", get(|| async { Redirect::temporary("/admin/") }));

    fallback.merge(api)
}
