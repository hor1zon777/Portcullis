//! 管理员操作审计（v1.5.0）。
//!
//! 记录关键操作：站点 CRUD / IP 封解 / manifest key 生成撤销 / 登录失败 / 密钥轮换。
//! 审计记录只存在 DB（不随 `log_file` 外泄），由管理员「审计」页查询。
//!
//! Token 前缀脱敏：`sha256(admin_token)[..4]` 的 hex（8 字符），确保同一把
//! token 的所有操作能被聚合追踪，但 DB 泄漏时也无法还原原始 token。

use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::Db;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub ts: u64,
    pub token_prefix: Option<String>,
    pub action: String,
    pub target: Option<String>,
    pub ip: Option<String>,
    pub success: bool,
    pub meta_json: Option<String>,
}

// ──────── 常量：action 字符串 ────────

pub const ACTION_SITE_CREATE: &str = "site.create";
pub const ACTION_SITE_UPDATE: &str = "site.update";
pub const ACTION_SITE_DELETE: &str = "site.delete";
pub const ACTION_IP_BLOCK: &str = "ip.block";
pub const ACTION_IP_UNBLOCK: &str = "ip.unblock";
pub const ACTION_MANIFEST_GENERATE: &str = "manifest.generate";
pub const ACTION_MANIFEST_REVOKE: &str = "manifest.revoke";
pub const ACTION_LOGIN_FAIL: &str = "login.fail";

// ──────── 脱敏工具 ────────

/// 从 `Authorization: Bearer xxx` 或 `?token=xxx` 提取 admin token
/// 并对其做 `sha256(token)[..4]` hex 脱敏，返回 8 字符前缀。
pub fn token_prefix_from_headers(headers: &HeaderMap) -> Option<String> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))?;
    Some(token_prefix(token))
}

pub fn token_prefix(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let h = hasher.finalize();
    format!("{:02x}{:02x}{:02x}{:02x}", h[0], h[1], h[2], h[3])
}

pub fn client_ip_from_headers(headers: &HeaderMap) -> Option<String> {
    crate::rate_limit::extract_ip(headers, None).map(|ip| ip.to_string())
}

// ──────── 写入 ────────

/// 同步写入一条审计记录（在 spawn_blocking 上下文中调用）。
pub fn record(
    db: &Db,
    token_prefix: Option<&str>,
    action: &str,
    target: Option<&str>,
    ip: Option<&str>,
    success: bool,
    meta_json: Option<&str>,
) {
    crate::db::insert_audit(db, token_prefix, action, target, ip, success, meta_json);
}

/// 异步写入一条审计记录（`tokio::spawn_blocking`），不阻塞 handler。
/// 同时触发 webhook 通知（如配置）。
pub fn spawn_record(
    state: &crate::state::AppState,
    token_prefix: Option<String>,
    action: &'static str,
    target: Option<String>,
    ip: Option<String>,
    success: bool,
    meta_json: Option<String>,
) {
    let db = state.db.clone();
    let webhook_url = state.config.load().admin_webhook_url.clone();
    let tp = token_prefix.clone();
    let t = target.clone();
    let ip_c = ip.clone();
    let mj = meta_json.clone();

    tokio::task::spawn_blocking(move || {
        record(&db, tp.as_deref(), action, t.as_deref(), ip_c.as_deref(), success, mj.as_deref());
    });

    // webhook fire-and-forget，失败仅记日志
    if let Some(url) = webhook_url {
        crate::admin::webhook::spawn_post(url, action, target, ip, success, meta_json);
    }
}

// ──────── 读取 ────────

pub fn load_recent(
    db: &Db,
    limit: usize,
    offset: usize,
    action_filter: Option<&str>,
) -> Vec<AuditEntry> {
    crate::db::load_recent_audit(db, limit, offset, action_filter)
}

pub fn count(db: &Db, action_filter: Option<&str>) -> i64 {
    crate::db::count_audit(db, action_filter)
}
