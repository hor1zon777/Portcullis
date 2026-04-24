use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

use crate::admin::audit;
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
    /// v1.5.0：`secret_key` 明文仅在创建时一次性返回，列表接口中固定为
    /// `"(hashed)"` 占位字符串，前端据此提醒用户务必在创建时保存原文。
    secret_key: String,
    diff: u8,
    origins: Vec<String>,
    argon2_m_cost: u32,
    argon2_t_cost: u32,
    argon2_p_cost: u32,
    bind_token_to_ip: bool,
    bind_token_to_ua: bool,
    secret_key_hashed: bool,
}

pub async fn list_sites(State(state): State<AppState>) -> Json<Vec<SiteView>> {
    let config = state.config.load();
    let sites: Vec<SiteView> = config
        .sites
        .iter()
        .map(|(k, v)| SiteView {
            key: k.clone(),
            secret_key: if v.secret_key_hashed {
                "(hashed)".to_string()
            } else {
                // 理论上启动迁移后不会出现；仅兜底
                v.secret_key.clone()
            },
            diff: v.diff,
            origins: v.origins.clone(),
            argon2_m_cost: v.argon2_m_cost,
            argon2_t_cost: v.argon2_t_cost,
            argon2_p_cost: v.argon2_p_cost,
            bind_token_to_ip: v.bind_token_to_ip,
            bind_token_to_ua: v.bind_token_to_ua,
            secret_key_hashed: v.secret_key_hashed,
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
    pub argon2_m_cost: Option<u32>,
    pub argon2_t_cost: Option<u32>,
    pub argon2_p_cost: Option<u32>,
    pub bind_token_to_ip: Option<bool>,
    pub bind_token_to_ua: Option<bool>,
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
    headers: HeaderMap,
    Json(req): Json<CreateSiteRequest>,
) -> Response {
    let token_prefix = audit::token_prefix_from_headers(&headers);
    let ip = audit::client_ip_from_headers(&headers);

    let mut config = (*state.config.load_full()).clone();

    let site_key = gen_site_key();
    let secret_key_plain = gen_secret_key();
    let secret_key_hash = crate::site_secret::hash(&secret_key_plain, &state.config.load().secret);
    let new_site = crate::config::SiteConfig {
        secret_key: secret_key_hash,
        diff: req.diff,
        origins: req.origins,
        argon2_m_cost: req
            .argon2_m_cost
            .unwrap_or(captcha_core::challenge::DEFAULT_M_COST),
        argon2_t_cost: req
            .argon2_t_cost
            .unwrap_or(captcha_core::challenge::DEFAULT_T_COST),
        argon2_p_cost: req
            .argon2_p_cost
            .unwrap_or(captcha_core::challenge::DEFAULT_P_COST),
        bind_token_to_ip: req.bind_token_to_ip.unwrap_or(false),
        bind_token_to_ua: req.bind_token_to_ua.unwrap_or(false),
        secret_key_hashed: true,
    };
    if let Err(e) = new_site.validate_argon2_params() {
        audit::spawn_record(
            &state,
            token_prefix,
            audit::ACTION_SITE_CREATE,
            None,
            ip,
            false,
            Some(format!(
                r#"{{"error":{}}}"#,
                serde_json::to_string(&e).unwrap_or_default()
            )),
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    crate::db::insert_site(&state.db, &site_key, &new_site);
    config.sites.insert(site_key.clone(), new_site);
    state.reload_config(config).await;

    audit::spawn_record(
        &state,
        token_prefix,
        audit::ACTION_SITE_CREATE,
        Some(site_key.clone()),
        ip,
        true,
        Some(format!(
            r#"{{"diff":{},"bind_ip":{},"bind_ua":{}}}"#,
            req.diff,
            req.bind_token_to_ip.unwrap_or(false),
            req.bind_token_to_ua.unwrap_or(false),
        )),
    );

    (
        StatusCode::CREATED,
        Json(serde_json::json!({"ok": true, "key": site_key, "secret_key": secret_key_plain})),
    )
        .into_response()
}

// ──────── DELETE /admin/api/sites/:key ────────

pub async fn delete_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    let token_prefix = audit::token_prefix_from_headers(&headers);
    let ip = audit::client_ip_from_headers(&headers);

    let mut config = (*state.config.load_full()).clone();
    if config.sites.remove(&key).is_none() {
        audit::spawn_record(
            &state,
            token_prefix,
            audit::ACTION_SITE_DELETE,
            Some(key),
            ip,
            false,
            Some(r#"{"error":"not_found"}"#.to_string()),
        );
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "站点不存在"})),
        )
            .into_response();
    }
    crate::db::delete_site(&state.db, &key);
    state.reload_config(config).await;

    audit::spawn_record(
        &state,
        token_prefix,
        audit::ACTION_SITE_DELETE,
        Some(key),
        ip,
        true,
        None,
    );

    Json(serde_json::json!({"ok": true})).into_response()
}

// ──────── PUT /admin/api/sites/:key ────────

#[derive(Deserialize)]
pub struct UpdateSiteRequest {
    pub diff: Option<u8>,
    #[serde(default)]
    pub origins: Option<Vec<String>>,
    pub argon2_m_cost: Option<u32>,
    pub argon2_t_cost: Option<u32>,
    pub argon2_p_cost: Option<u32>,
    pub bind_token_to_ip: Option<bool>,
    pub bind_token_to_ua: Option<bool>,
}

pub async fn update_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
    Json(req): Json<UpdateSiteRequest>,
) -> Response {
    let token_prefix = audit::token_prefix_from_headers(&headers);
    let ip = audit::client_ip_from_headers(&headers);

    let mut config = (*state.config.load_full()).clone();
    let Some(site) = config.sites.get_mut(&key) else {
        audit::spawn_record(
            &state,
            token_prefix,
            audit::ACTION_SITE_UPDATE,
            Some(key),
            ip,
            false,
            Some(r#"{"error":"not_found"}"#.to_string()),
        );
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
    if let Some(m) = req.argon2_m_cost {
        site.argon2_m_cost = m;
    }
    if let Some(t) = req.argon2_t_cost {
        site.argon2_t_cost = t;
    }
    if let Some(p) = req.argon2_p_cost {
        site.argon2_p_cost = p;
    }
    if let Some(b) = req.bind_token_to_ip {
        site.bind_token_to_ip = b;
    }
    if let Some(b) = req.bind_token_to_ua {
        site.bind_token_to_ua = b;
    }
    if let Err(e) = site.validate_argon2_params() {
        audit::spawn_record(
            &state,
            token_prefix,
            audit::ACTION_SITE_UPDATE,
            Some(key),
            ip,
            false,
            Some(format!(
                r#"{{"error":{}}}"#,
                serde_json::to_string(&e).unwrap_or_default()
            )),
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    crate::db::update_site_fields(
        &state.db,
        &key,
        req.diff,
        req.origins.as_deref(),
        req.argon2_m_cost,
        req.argon2_t_cost,
        req.argon2_p_cost,
        req.bind_token_to_ip,
        req.bind_token_to_ua,
    );
    state.reload_config(config).await;

    audit::spawn_record(
        &state,
        token_prefix,
        audit::ACTION_SITE_UPDATE,
        Some(key),
        ip,
        true,
        None,
    );

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

pub async fn block_ip(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<BlockRequest>,
) -> Response {
    let token_prefix = audit::token_prefix_from_headers(&headers);
    let ip_ = audit::client_ip_from_headers(&headers);

    let mut risk = state.risk.write().await;
    if risk.add_blocked(&req.ip) {
        crate::db::insert_ip_list(&state.db, &req.ip, "blocked");
        tracing::info!(ip = %req.ip, "管理员封禁 IP");
        drop(risk);
        audit::spawn_record(
            &state,
            token_prefix,
            audit::ACTION_IP_BLOCK,
            Some(req.ip.clone()),
            ip_,
            true,
            None,
        );
        Json(serde_json::json!({"ok": true})).into_response()
    } else {
        drop(risk);
        audit::spawn_record(
            &state,
            token_prefix,
            audit::ACTION_IP_BLOCK,
            Some(req.ip.clone()),
            ip_,
            false,
            Some(r#"{"error":"invalid_or_duplicate"}"#.to_string()),
        );
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "无效 IP 或已在黑名单"})),
        )
            .into_response()
    }
}

// ──────── DELETE /admin/api/risk/block ────────

pub async fn unblock_ip(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<BlockRequest>,
) -> Response {
    let token_prefix = audit::token_prefix_from_headers(&headers);
    let ip_ = audit::client_ip_from_headers(&headers);

    let mut risk = state.risk.write().await;
    if risk.remove_blocked(&req.ip) {
        crate::db::delete_ip_list(&state.db, &req.ip, "blocked");
        tracing::info!(ip = %req.ip, "管理员解封 IP");
        drop(risk);
        audit::spawn_record(
            &state,
            token_prefix,
            audit::ACTION_IP_UNBLOCK,
            Some(req.ip.clone()),
            ip_,
            true,
            None,
        );
        Json(serde_json::json!({"ok": true})).into_response()
    } else {
        drop(risk);
        audit::spawn_record(
            &state,
            token_prefix,
            audit::ACTION_IP_UNBLOCK,
            Some(req.ip.clone()),
            ip_,
            false,
            Some(r#"{"error":"not_found"}"#.to_string()),
        );
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "IP 不在黑名单中"})),
        )
            .into_response()
    }
}

// ──────── GET /admin/api/manifest-pubkey ────────

#[derive(Serialize)]
pub struct ManifestPubkeyResponse {
    /// 是否已配置 manifest 签名私钥
    enabled: bool,
    /// base64 公钥；`enabled=false` 时省略
    #[serde(skip_serializing_if = "Option::is_none")]
    pubkey: Option<String>,
    /// 对应算法，固定 "ed25519"，便于未来扩展
    algorithm: &'static str,
}

pub async fn manifest_pubkey(State(state): State<AppState>) -> Json<ManifestPubkeyResponse> {
    let cfg = state.config.load();
    match cfg.manifest_signing_key {
        Some(seed) => {
            let sk = SigningKey::from_bytes(&seed);
            let pk = sk.verifying_key();
            Json(ManifestPubkeyResponse {
                enabled: true,
                pubkey: Some(B64.encode(pk.to_bytes())),
                algorithm: "ed25519",
            })
        }
        None => Json(ManifestPubkeyResponse {
            enabled: false,
            pubkey: None,
            algorithm: "ed25519",
        }),
    }
}

// ──────── POST /admin/api/manifest-pubkey/generate ────────

#[derive(Serialize)]
pub struct GenerateManifestKeyResponse {
    enabled: bool,
    pubkey: String,
    algorithm: &'static str,
    /// 是否为首次生成（false 表示覆盖了已有密钥）
    first_time: bool,
}

pub async fn generate_manifest_key(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let token_prefix = audit::token_prefix_from_headers(&headers);
    let ip = audit::client_ip_from_headers(&headers);

    let first_time = {
        let cfg = state.config.load();
        cfg.manifest_signing_key.is_none()
    };

    let mut seed = [0u8; 32];
    if let Err(e) = getrandom::getrandom(&mut seed) {
        tracing::error!("manifest 签名 seed 生成失败: {e}");
        audit::spawn_record(
            &state,
            token_prefix,
            audit::ACTION_MANIFEST_GENERATE,
            None,
            ip,
            false,
            Some(r#"{"error":"rng_failed"}"#.to_string()),
        );
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "随机数生成失败"})),
        )
            .into_response();
    }

    crate::db::save_server_secret_32(&state.db, "manifest_signing_key", &seed);

    // 更新 ArcSwap 配置，使后续 /sdk/manifest.json 立即用新 key 签名
    let mut new_cfg = state.config.load_full().as_ref().clone();
    new_cfg.manifest_signing_key = Some(seed);
    state.config.store(std::sync::Arc::new(new_cfg));

    let pk = SigningKey::from_bytes(&seed).verifying_key();
    tracing::info!(first_time, "管理面板生成新的 manifest 签名密钥");

    audit::spawn_record(
        &state,
        token_prefix,
        audit::ACTION_MANIFEST_GENERATE,
        None,
        ip,
        true,
        Some(format!(r#"{{"first_time":{first_time}}}"#)),
    );

    Json(GenerateManifestKeyResponse {
        enabled: true,
        pubkey: B64.encode(pk.to_bytes()),
        algorithm: "ed25519",
        first_time,
    })
    .into_response()
}

// ──────── DELETE /admin/api/manifest-pubkey ────────

pub async fn revoke_manifest_key(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let token_prefix = audit::token_prefix_from_headers(&headers);
    let ip = audit::client_ip_from_headers(&headers);

    let removed = crate::db::delete_server_secret(&state.db, "manifest_signing_key");

    // 热生效：把配置中的 signing key 置空
    let mut new_cfg = state.config.load_full().as_ref().clone();
    new_cfg.manifest_signing_key = None;
    state.config.store(std::sync::Arc::new(new_cfg));

    audit::spawn_record(
        &state,
        token_prefix,
        audit::ACTION_MANIFEST_REVOKE,
        None,
        ip,
        removed,
        Some(format!(r#"{{"removed":{removed}}}"#)),
    );

    if removed {
        tracing::info!("管理面板撤销 manifest 签名密钥");
        Json(serde_json::json!({"ok": true, "removed": true})).into_response()
    } else {
        // 没有密钥可撤销，也算幂等成功
        Json(serde_json::json!({"ok": true, "removed": false})).into_response()
    }
}

// ──────── GET /admin/api/audit（v1.5.0） ────────

#[derive(Deserialize)]
pub struct AuditQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub action: Option<String>,
}

#[derive(Serialize)]
pub struct AuditListResponse {
    pub total: i64,
    pub entries: Vec<audit::AuditEntry>,
}

pub async fn audit_list(
    State(state): State<AppState>,
    Query(q): Query<AuditQuery>,
) -> Json<AuditListResponse> {
    let limit = q.limit.unwrap_or(100).min(500);
    let offset = q.offset.unwrap_or(0);
    let action_filter = q.action.clone();
    let db = state.db.clone();

    let (total, entries) = tokio::task::spawn_blocking(move || {
        let total = audit::count(&db, action_filter.as_deref());
        let entries = audit::load_recent(&db, limit, offset, action_filter.as_deref());
        (total, entries)
    })
    .await
    .unwrap_or_else(|_| (0, Vec::new()));

    Json(AuditListResponse { total, entries })
}
