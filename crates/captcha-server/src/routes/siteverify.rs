use std::net::IpAddr;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use crate::error::AppError;
use crate::state::AppState;
use crate::token;

#[derive(Debug, Deserialize)]
pub struct SiteVerifyRequest {
    pub token: String,
    pub secret_key: String,
    /// v1.4.0：启用 `bind_token_to_ip` 的站点必须提供，否则拒绝。
    /// 传入形式为 IP 字符串（IPv4 或 IPv6）。
    #[serde(default)]
    pub client_ip: Option<String>,
    /// v1.4.0：启用 `bind_token_to_ua` 的站点必须提供（原串）。
    #[serde(default)]
    pub user_agent: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SiteVerifyResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub challenge_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn fail(msg: &str) -> Json<SiteVerifyResponse> {
    crate::metrics::record_siteverify(false);
    Json(SiteVerifyResponse {
        success: false,
        challenge_id: None,
        site_key: None,
        error: Some(msg.to_string()),
    })
}

/// POST /api/v1/siteverify
pub async fn site_verify(
    State(state): State<AppState>,
    Json(req): Json<SiteVerifyRequest>,
) -> Result<Json<SiteVerifyResponse>, AppError> {
    // 1. 验证 token 签名 + 过期
    let cfg = state.config.load();
    let verified = match token::verify_full(&req.token, &cfg.secret) {
        Some(v) => v,
        None => return Ok(fail("token 无效或已过期")),
    };

    // 2. 常数时间比较 secret_key
    let site = match cfg.get_site(&verified.site_key) {
        Some(s) => s,
        None => return Ok(fail("site_key 已下线")),
    };
    let expected = site.secret_key.as_bytes();
    let provided = req.secret_key.as_bytes();
    let len_match = expected.len() == provided.len();
    let content_match: bool = if len_match {
        expected.ct_eq(provided).into()
    } else {
        false
    };
    if !content_match {
        return Ok(fail("secret_key 不匹配"));
    }

    // 3. v1.4.0 身份绑定校验：仅当 token 自身携带 hash 时强制比对。
    //    这样即使站点管理员关掉了绑定，之前发放的 token 仍按发放时策略生效，
    //    避免热切换时用户请求被意外拒绝。
    if let Some(expected_ip_hash) = verified.ip_hash {
        let Some(ref ip_str) = req.client_ip else {
            return Ok(fail("token 要求 IP 绑定，但 siteverify 未携带 client_ip"));
        };
        let ip: IpAddr = match ip_str.parse() {
            Ok(ip) => ip,
            Err(_) => return Ok(fail("client_ip 不是合法的 IP 地址")),
        };
        let actual = token::hash_ip(&ip);
        if !token::ip_hash_eq(&expected_ip_hash, &actual) {
            return Ok(fail("client_ip 与 token 绑定不一致"));
        }
    }

    if let Some(expected_ua_hash) = verified.ua_hash {
        let Some(ref ua) = req.user_agent else {
            return Ok(fail("token 要求 UA 绑定，但 siteverify 未携带 user_agent"));
        };
        let actual = token::hash_ua(ua);
        if !token::ua_hash_eq(&expected_ua_hash, &actual) {
            return Ok(fail("user_agent 与 token 绑定不一致"));
        }
    }

    // 4. token 单次使用（同一 challenge_id 只能核验一次）
    if !state
        .store
        .mark_token_used(&verified.challenge_id, verified.exp)
    {
        return Ok(fail("token 已被核验过（单次使用）"));
    }
    {
        let db = state.db.clone();
        let cid = verified.challenge_id.clone();
        let exp = verified.exp;
        tokio::task::spawn_blocking(move || {
            crate::db::mark_nonce_used(&db, &cid, "token", exp);
        });
    }

    crate::metrics::record_siteverify(true);
    Ok(Json(SiteVerifyResponse {
        success: true,
        challenge_id: Some(verified.challenge_id),
        site_key: Some(verified.site_key),
        error: None,
    }))
}
