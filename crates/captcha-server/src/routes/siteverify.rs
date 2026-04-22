use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

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

/// POST /api/v1/siteverify
/// 业务后端用自己的 secret_key 核验 captcha_token。
/// 设计上不返回 4xx，而是用 body.success=false 告知失败原因，
/// 便于业务后端统一处理。
pub async fn site_verify(
    State(state): State<AppState>,
    Json(req): Json<SiteVerifyRequest>,
) -> Result<Json<SiteVerifyResponse>, AppError> {
    let (challenge_id, site_key) = match token::verify(&req.token, &state.config.secret) {
        Some(v) => v,
        None => {
            return Ok(Json(SiteVerifyResponse {
                success: false,
                challenge_id: None,
                site_key: None,
                error: Some("token 无效或已过期".to_string()),
            }));
        }
    };

    // 校验 secret_key 与 site_key 绑定
    match state.config.get_site(&site_key) {
        Some(s) if s.secret_key == req.secret_key => {}
        Some(_) => {
            return Ok(Json(SiteVerifyResponse {
                success: false,
                challenge_id: None,
                site_key: None,
                error: Some("secret_key 不匹配".to_string()),
            }));
        }
        None => {
            return Ok(Json(SiteVerifyResponse {
                success: false,
                challenge_id: None,
                site_key: None,
                error: Some("site_key 已下线".to_string()),
            }));
        }
    }

    Ok(Json(SiteVerifyResponse {
        success: true,
        challenge_id: Some(challenge_id),
        site_key: Some(site_key),
        error: None,
    }))
}
