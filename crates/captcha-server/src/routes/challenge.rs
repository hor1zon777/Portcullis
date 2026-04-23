use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use captcha_core::{challenge::Challenge, crypto};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::rate_limit::extract_ip;
use crate::state::AppState;

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

pub(crate) fn check_origin(headers: &HeaderMap, origins: &[String]) -> Result<(), AppError> {
    if origins.is_empty() {
        return Ok(());
    }
    let Some(origin_value) = headers.get("origin").and_then(|v| v.to_str().ok()) else {
        return Ok(());
    };
    if origins.iter().any(|o| o == origin_value) {
        Ok(())
    } else {
        tracing::warn!(origin = %origin_value, "Origin 不在白名单内");
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

    let config = state.config.load();
    let site = config
        .get_site(&req.site_key)
        .ok_or_else(|| AppError::BadRequest(format!("未知的 site_key: {}", req.site_key)))?;

    check_origin(&headers, &site.origins)?;

    // IP 黑名单检查
    let client_ip = extract_ip(&headers, None);
    if let Some(ip) = client_ip {
        let risk = state.risk.read().await;
        if risk.is_blocked(ip) {
            return Err(AppError::Unauthorized("IP 已被封禁".to_string()));
        }
    }

    // 动态难度
    let extra_diff = if let Some(ip) = client_ip {
        let risk = state.risk.read().await;
        risk.extra_diff(ip)
    } else {
        0
    };
    let effective_diff = site.diff.saturating_add(extra_diff);

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
        diff: effective_diff,
        exp: now_ms + config.challenge_ttl_secs * 1000,
        site_key: req.site_key,
        m_cost: site.argon2_m_cost,
        t_cost: site.argon2_t_cost,
        p_cost: site.argon2_p_cost,
    };

    let sig_bytes = crypto::sign(&challenge.to_sign_bytes(), &config.secret);

    crate::metrics::record_challenge_issued(&challenge.site_key);

    if extra_diff > 0 {
        tracing::info!(
            challenge_id = %challenge.id,
            base_diff = site.diff,
            extra_diff,
            effective_diff,
            "动态难度提升"
        );
    }

    Ok(Json(ChallengeResponse {
        success: true,
        challenge,
        sig: B64.encode(sig_bytes),
    }))
}
