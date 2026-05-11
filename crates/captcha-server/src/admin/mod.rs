pub mod audit;
pub mod auth;
pub mod handlers;
pub mod path_check;
pub mod request_log;
pub mod webhook;

use axum::response::Redirect;
use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::state::AppState;

pub fn admin_router(state: AppState, _token: String) -> Router {
    // v1.6.0：所有 admin API 路径都带一个随机 `:suffix` 段。
    // 实际值在启动期生成并保存在 DB；`path_check::admin_path_middleware`
    // 负责常数时间比对，错误 suffix 一律 404。
    let api = Router::new()
        .route("/admin/:suffix/api/stats", get(handlers::stats))
        .route("/admin/:suffix/api/sites", get(handlers::list_sites))
        .route("/admin/:suffix/api/sites", post(handlers::create_site))
        .route("/admin/:suffix/api/sites/:key", put(handlers::update_site))
        .route(
            "/admin/:suffix/api/sites/:key",
            delete(handlers::delete_site),
        )
        .route("/admin/:suffix/api/logs", get(handlers::logs))
        .route("/admin/:suffix/api/risk/ips", get(handlers::risk_ips))
        .route("/admin/:suffix/api/risk/block", post(handlers::block_ip))
        .route(
            "/admin/:suffix/api/risk/block",
            delete(handlers::unblock_ip),
        )
        .route(
            "/admin/:suffix/api/manifest-pubkey",
            get(handlers::manifest_pubkey),
        )
        .route(
            "/admin/:suffix/api/manifest-pubkey/generate",
            post(handlers::generate_manifest_key),
        )
        .route(
            "/admin/:suffix/api/manifest-pubkey",
            delete(handlers::revoke_manifest_key),
        )
        .route("/admin/:suffix/api/audit", get(handlers::audit_list))
        // v1.6.0：暴露给 admin UI 自身查看 / rotate / 自定义 admin path
        .route(
            "/admin/:suffix/api/admin-path",
            get(handlers::admin_path_get),
        )
        .route(
            "/admin/:suffix/api/admin-path",
            put(handlers::admin_path_update),
        )
        .route(
            "/admin/:suffix/api/admin-path/rotate",
            post(handlers::admin_path_rotate),
        )
        .with_state(state.clone())
        // 顺序说明（axum 的 .layer 是从下往上"包裹"，所以执行顺序是从上往下）：
        //  1. path_check.admin_path_middleware  — 错误 suffix 直接 404，最先拦截
        //  2. auth.auth_middleware_with_state   — token 鉴权 + 失败 ban
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware_with_state,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state,
            path_check::admin_path_middleware,
        ));

    // /admin 页面由 admin-ui 容器（Nginx）提供；
    // 单二进制部署时重定向到提示页
    let fallback = Router::new().route("/admin", get(|| async { Redirect::temporary("/admin/") }));

    fallback.merge(api)
}
