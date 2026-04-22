use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use captcha_core::{challenge::Challenge, crypto, pow};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::routes::challenge::check_origin;
use crate::state::AppState;
use crate::token;

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub challenge: Challenge,
    pub sig: String,
    pub nonce: u64,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub success: bool,
    pub captcha_token: String,
    pub exp: u64,
}

/// POST /api/v1/verify
pub async fn verify(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, AppError> {
    // 0. Origin 白名单校验
    if let Some(site) = state.config.get_site(&req.challenge.site_key) {
        check_origin(&headers, &site.origins)?;
    }

    // 1. 签名
    let sig_vec = B64
        .decode(&req.sig)
        .map_err(|_| AppError::BadRequest("sig base64 解码失败".to_string()))?;
    let sig_arr: [u8; 32] = sig_vec
        .as_slice()
        .try_into()
        .map_err(|_| AppError::BadRequest("sig 长度必须为 32 字节".to_string()))?;

    if !crypto::verify_sig(
        &req.challenge.to_sign_bytes(),
        &sig_arr,
        &state.config.secret,
    ) {
        return Err(AppError::Unauthorized("签名验证失败".to_string()));
    }

    // 2. 过期
    if req.challenge.is_expired() {
        return Err(AppError::BadRequest("挑战已过期".to_string()));
    }

    // 3. 防重放
    if !state
        .store
        .mark_challenge_used(&req.challenge.id, req.challenge.exp)
    {
        return Err(AppError::Conflict("挑战已被使用".to_string()));
    }

    // 4. PoW 验证
    if !pow::verify_solution(&req.challenge, req.nonce) {
        return Err(AppError::BadRequest("PoW 解答不满足难度要求".to_string()));
    }

    // 5. 生成 captcha_token
    let (ct_token, exp) = token::generate(
        &req.challenge.id,
        &req.challenge.site_key,
        state.config.token_ttl_secs,
        &state.config.secret,
    );

    tracing::info!(
        challenge_id = %req.challenge.id,
        site_key = %req.challenge.site_key,
        "挑战验证成功"
    );

    Ok(Json(VerifyResponse {
        success: true,
        captcha_token: ct_token,
        exp,
    }))
}
