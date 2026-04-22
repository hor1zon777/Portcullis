use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

// ──────── GET /admin/api/stats ────────

#[derive(Serialize)]
pub struct StatsResponse {
    store: crate::store::memory::StoreMetrics,
    risk_ips_tracked: usize,
    request_log_count: usize,
    sites_count: usize,
    uptime_note: &'static str,
}

pub async fn stats(State(state): State<AppState>) -> Json<StatsResponse> {
    let store_metrics = state.store.metrics();
    let risk = state.risk.read().await;
    let ip_count = risk.ip_summary().len();
    let log_count = state.request_log.len();
    let config = state.config.load();

    Json(StatsResponse {
        store: store_metrics,
        risk_ips_tracked: ip_count,
        request_log_count: log_count,
        sites_count: config.sites.len(),
        uptime_note: "use /metrics for detailed Prometheus data",
    })
}

// ──────── GET /admin/api/sites ────────

#[derive(Serialize)]
pub struct SiteView {
    key: String,
    secret_key: String,
    diff: u8,
    origins: Vec<String>,
}

pub async fn list_sites(State(state): State<AppState>) -> Json<Vec<SiteView>> {
    let config = state.config.load();
    let sites: Vec<SiteView> = config
        .sites
        .iter()
        .map(|(k, v)| SiteView {
            key: k.clone(),
            secret_key: v.secret_key.clone(),
            diff: v.diff,
            origins: v.origins.clone(),
        })
        .collect();
    Json(sites)
}

// ──────── POST /admin/api/sites ────────

#[derive(Deserialize)]
pub struct CreateSiteRequest {
    pub diff: u8,
    #[serde(default)]
    pub origins: Vec<String>,
}

fn gen_hex(len: usize) -> String {
    let mut buf = vec![0u8; len];
    getrandom::getrandom(&mut buf).expect("随机数生成失败");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn gen_site_key() -> String {
    format!("pk_{}", gen_hex(12))
}

fn gen_secret_key() -> String {
    gen_hex(32)
}

pub async fn create_site(
    State(state): State<AppState>,
    Json(req): Json<CreateSiteRequest>,
) -> Response {
    let mut config = (*state.config.load_full()).clone();

    let site_key = gen_site_key();
    let secret_key = gen_secret_key();
    let new_site = crate::config::SiteConfig {
        secret_key: secret_key.clone(),
        diff: req.diff,
        origins: req.origins,
    };
    crate::db::insert_site(&state.db, &site_key, &new_site);
    config.sites.insert(site_key.clone(), new_site);
    state.reload_config(config).await;
    (
        StatusCode::CREATED,
        Json(serde_json::json!({"ok": true, "key": site_key, "secret_key": secret_key})),
    )
        .into_response()
}

// ──────── DELETE /admin/api/sites/:key ────────

pub async fn delete_site(State(state): State<AppState>, Path(key): Path<String>) -> Response {
    let mut config = (*state.config.load_full()).clone();
    if config.sites.remove(&key).is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "站点不存在"})),
        )
            .into_response();
    }
    crate::db::delete_site(&state.db, &key);
    state.reload_config(config).await;
    Json(serde_json::json!({"ok": true})).into_response()
}

// ──────── PUT /admin/api/sites/:key ────────

#[derive(Deserialize)]
pub struct UpdateSiteRequest {
    pub diff: Option<u8>,
    #[serde(default)]
    pub origins: Option<Vec<String>>,
}

pub async fn update_site(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(req): Json<UpdateSiteRequest>,
) -> Response {
    let mut config = (*state.config.load_full()).clone();
    let Some(site) = config.sites.get_mut(&key) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "站点不存在"})),
        )
            .into_response();
    };
    if let Some(d) = req.diff {
        site.diff = d;
    }
    if let Some(ref o) = req.origins {
        site.origins = o.clone();
    }
    crate::db::update_site_fields(&state.db, &key, req.diff, req.origins.as_deref());
    state.reload_config(config).await;
    Json(serde_json::json!({"ok": true})).into_response()
}

// ──────── GET /admin/api/logs ────────

pub async fn logs(State(state): State<AppState>) -> Json<Vec<super::request_log::LogEntry>> {
    let db = state.db.clone();
    let result = tokio::task::spawn_blocking(move || crate::db::load_recent_logs(&db, 200))
        .await
        .unwrap_or_default();
    Json(result)
}

// ──────── GET /admin/api/risk/ips ────────

pub async fn risk_ips(State(state): State<AppState>) -> Json<serde_json::Value> {
    let risk = state.risk.read().await;
    let ips = risk.ip_summary();
    let blocked = risk.blocked_list();
    let allowed = risk.allowed_list();
    Json(serde_json::json!({
        "ips": ips,
        "blocked": blocked,
        "allowed": allowed,
    }))
}

// ──────── POST /admin/api/risk/block ────────

#[derive(Deserialize)]
pub struct BlockRequest {
    pub ip: String,
}

pub async fn block_ip(State(state): State<AppState>, Json(req): Json<BlockRequest>) -> Response {
    let mut risk = state.risk.write().await;
    if risk.add_blocked(&req.ip) {
        crate::db::insert_ip_list(&state.db, &req.ip, "blocked");
        tracing::info!(ip = %req.ip, "管理员封禁 IP");
        Json(serde_json::json!({"ok": true})).into_response()
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "无效 IP 或已在黑名单"})),
        )
            .into_response()
    }
}

// ──────── DELETE /admin/api/risk/block ────────

pub async fn unblock_ip(State(state): State<AppState>, Json(req): Json<BlockRequest>) -> Response {
    let mut risk = state.risk.write().await;
    if risk.remove_blocked(&req.ip) {
        crate::db::delete_ip_list(&state.db, &req.ip, "blocked");
        tracing::info!(ip = %req.ip, "管理员解封 IP");
        Json(serde_json::json!({"ok": true})).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "IP 不在黑名单中"})),
        )
            .into_response()
    }
}
