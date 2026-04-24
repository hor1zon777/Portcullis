use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use captcha_core::{challenge::Challenge, crypto, pow};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::rate_limit::extract_ip;
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
    let started = std::time::Instant::now();
    let site_key = req.challenge.site_key.clone();
    let nonce = req.nonce;
    let client_ip = extract_ip(&headers, None);

    let result = do_verify(&state, &headers, req).await;

    let success = result.is_ok();
    let duration_ms = started.elapsed().as_secs_f64() * 1000.0;
    crate::metrics::record_verify(&site_key, success, started);

    // 写入请求日志（内存 + DB）
    let log_entry = crate::admin::request_log::LogEntry {
        timestamp: crate::admin::request_log::now_ms(),
        ip: client_ip,
        site_key: site_key.clone(),
        nonce,
        success,
        duration_ms,
        error: if success {
            None
        } else {
            Some("verify failed".to_string())
        },
    };
    state.request_log.inc();
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || crate::db::insert_log(&db, &log_entry));

    // 记录风控数据
    if let Some(ip) = client_ip {
        state.risk.read().await.record_verify(ip, success);
    }

    result
}

async fn do_verify(
    state: &AppState,
    headers: &HeaderMap,
    req: VerifyRequest,
) -> Result<Json<VerifyResponse>, AppError> {
    let config = state.config.load();

    if let Some(site) = config.get_site(&req.challenge.site_key) {
        check_origin(headers, &site.origins)?;
    }

    let sig_vec = B64
        .decode(&req.sig)
        .map_err(|_| AppError::BadRequest("sig base64 解码失败".to_string()))?;
    let sig_arr: [u8; 32] = sig_vec
        .as_slice()
        .try_into()
        .map_err(|_| AppError::BadRequest("sig 长度必须为 32 字节".to_string()))?;

    if !crypto::verify_sig_any(
        &req.challenge.to_sign_bytes(),
        &sig_arr,
        &config.verify_secrets(),
    ) {
        return Err(AppError::Unauthorized("签名验证失败".to_string()));
    }

    if req.challenge.is_expired() {
        return Err(AppError::BadRequest("挑战已过期".to_string()));
    }

    if !state
        .store
        .mark_challenge_used(&req.challenge.id, req.challenge.exp)
    {
        return Err(AppError::Conflict("挑战已被使用".to_string()));
    }
    // 同步写入 DB 防重放（重启后仍有效）
    {
        let db = state.db.clone();
        let id = req.challenge.id.clone();
        let exp = req.challenge.exp;
        tokio::task::spawn_blocking(move || {
            crate::db::mark_nonce_used(&db, &id, "challenge", exp);
        });
    }

    if !pow::verify_solution(&req.challenge, req.nonce) {
        return Err(AppError::BadRequest("PoW 解答不满足难度要求".to_string()));
    }

    // v1.4.0 身份绑定：按 site 开关在 token 里填入 IP / UA hash。
    // 未开启的 site 行为与 v1.3.x 完全一致，hash 字段不进 payload。
    let (ip_hash, ua_hash) = {
        let site = config.get_site(&req.challenge.site_key);
        let bind_ip = site.map(|s| s.bind_token_to_ip).unwrap_or(false);
        let bind_ua = site.map(|s| s.bind_token_to_ua).unwrap_or(false);
        let ip_hash = if bind_ip {
            match extract_ip(headers, None) {
                Some(ip) => Some(token::hash_ip(&ip)),
                None => {
                    return Err(AppError::BadRequest(
                        "站点启用了 IP 绑定，但无法识别 client IP（检查反向代理 X-Forwarded-For / X-Real-IP 透传）"
                            .to_string(),
                    ));
                }
            }
        } else {
            None
        };
        let ua_hash = if bind_ua {
            let ua = headers
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            Some(token::hash_ua(ua))
        } else {
            None
        };
        (ip_hash, ua_hash)
    };

    let (ct_token, exp) = token::generate(
        &req.challenge.id,
        &req.challenge.site_key,
        config.token_ttl_secs,
        &config.secret,
        ip_hash,
        ua_hash,
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

// ──────── batch verify ────────

#[derive(Debug, Deserialize)]
pub struct BatchVerifyRequest {
    pub items: Vec<VerifyRequest>,
}

#[derive(Debug, Serialize)]
pub struct BatchVerifyResponse {
    pub results: Vec<BatchVerifyItem>,
}

#[derive(Debug, Serialize)]
pub struct BatchVerifyItem {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captcha_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// POST /api/v1/verify/batch
pub async fn verify_batch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<BatchVerifyRequest>,
) -> Result<Json<BatchVerifyResponse>, AppError> {
    if req.items.len() > 20 {
        return Err(AppError::BadRequest("batch 最多 20 条".to_string()));
    }

    let mut results = Vec::with_capacity(req.items.len());
    for item in req.items {
        let site_key = item.challenge.site_key.clone();
        let started = std::time::Instant::now();

        match do_verify(&state, &headers, item).await {
            Ok(Json(v)) => {
                crate::metrics::record_verify(&site_key, true, started);
                results.push(BatchVerifyItem {
                    success: true,
                    captcha_token: Some(v.captcha_token),
                    error: None,
                });
            }
            Err(e) => {
                crate::metrics::record_verify(&site_key, false, started);
                results.push(BatchVerifyItem {
                    success: false,
                    captcha_token: None,
                    error: Some(format!("{e:?}")),
                });
            }
        }
    }

    Ok(Json(BatchVerifyResponse { results }))
}
