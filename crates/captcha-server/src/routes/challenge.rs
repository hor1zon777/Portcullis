use axum::extract::State;
use axum::Json;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use captcha_core::{challenge::Challenge, crypto};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ChallengeRequest {
    pub site_key: String,
}

#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    pub success: bool,
    pub challenge: Challenge,
    /// 服务端对 challenge 的 HMAC 签名，base64 标准编码
    pub sig: String,
}

/// POST /api/v1/challenge
/// 按 site_key 的配置发放新挑战，完全无状态（未写入 store）。
pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>, AppError> {
    let site = state
        .config
        .get_site(&req.site_key)
        .ok_or_else(|| AppError::BadRequest(format!("未知的 site_key: {}", req.site_key)))?;

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
