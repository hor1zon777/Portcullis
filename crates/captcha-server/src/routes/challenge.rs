use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use captcha_core::{challenge::Challenge, crypto};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::state::AppState;

/// site_key 最大长度（字节），防止恶意超长输入
const MAX_SITE_KEY_LEN: usize = 64;

#[derive(Debug, Deserialize)]
pub struct ChallengeRequest {
    pub site_key: String,
}

#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    pub success: bool,
    pub challenge: Challenge,
    pub sig: String,
}

/// 校验 Origin header 与该站点配置的白名单是否匹配。
/// - 若 Origin header 不存在（服务端到服务端调用）：放行
/// - 若 site.origins 为空（未配置白名单）：放行
/// - 否则必须精确匹配
pub(crate) fn check_origin(
    headers: &HeaderMap,
    origins: &[String],
) -> Result<(), AppError> {
    if origins.is_empty() {
        return Ok(());
    }
    let Some(origin_value) = headers.get("origin").and_then(|v| v.to_str().ok()) else {
        return Ok(());
    };
    if origins.iter().any(|o| o == origin_value) {
        Ok(())
    } else {
        tracing::warn!(origin = %origin_value, "Origin 不在白名单内，拒绝请求");
        Err(AppError::Unauthorized(format!(
            "Origin '{}' 不在白名单",
            origin_value
        )))
    }
}

/// POST /api/v1/challenge
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>, AppError> {
    if req.site_key.len() > MAX_SITE_KEY_LEN {
        return Err(AppError::BadRequest(format!(
            "site_key 过长（> {} 字节）",
            MAX_SITE_KEY_LEN
        )));
    }

    let site = state
        .config
        .get_site(&req.site_key)
        .ok_or_else(|| AppError::BadRequest(format!("未知的 site_key: {}", req.site_key)))?;

    check_origin(&headers, &site.origins)?;

    let mut salt = [0u8; 16];
    getrandom::getrandom(&mut salt)
        .map_err(|e| AppError::Internal(format!("随机数生成失败: {e}")))?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AppError::Internal("系统时钟异常".to_string()))?
        .as_millis() as u64;

    let challenge = Challenge {
        id: uuid::Uuid::new_v4().to_string(),
        salt,
        diff: site.diff,
        exp: now_ms + state.config.challenge_ttl_secs * 1000,
        site_key: req.site_key,
    };

    let sig_bytes = crypto::sign(&challenge.to_sign_bytes(), &state.config.secret);

    crate::metrics::record_challenge_issued(&challenge.site_key);

    tracing::debug!(
        challenge_id = %challenge.id,
        site_key = %challenge.site_key,
        diff = challenge.diff,
        "发放挑战"
    );

    Ok(Json(ChallengeResponse {
        success: true,
        challenge,
        sig: B64.encode(sig_bytes),
    }))
}
