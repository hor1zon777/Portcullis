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
    let (challenge_id, site_key, token_exp) =
        match token::verify_with_exp(&req.token, &cfg.secret) {
            Some(v) => v,
            None => return Ok(fail("token 无效或已过期")),
        };

    // 2. 常数时间比较 secret_key
    let site = match cfg.get_site(&site_key) {
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

    // 3. token 单次使用（同一 challenge_id 只能核验一次）
    if !state.store.mark_token_used(&challenge_id, token_exp) {
        return Ok(fail("token 已被核验过（单次使用）"));
    }

    crate::metrics::record_siteverify(true);
    Ok(Json(SiteVerifyResponse {
        success: true,
        challenge_id: Some(challenge_id),
        site_key: Some(site_key),
        error: None,
    }))
}
