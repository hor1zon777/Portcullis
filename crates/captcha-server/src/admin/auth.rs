use axum::extract::{Query, State};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

use crate::state::AppState;

#[derive(serde::Deserialize)]
pub struct TokenQuery {
    pub token: Option<String>,
}

/// v1.5.0：携带 `AppState` 的认证 middleware。期望 token 从 `state.config.admin_token` 读取。
///
/// 失败时：
/// 1. 记录 `login.fail` 审计（含 IP、脱敏 token 前缀）
/// 2. 触发 admin 登录限流：连续 30 次失败后临时 ban 15 分钟，返回 429
///
/// 成功时不写 audit（太频繁），具体操作由各 handler 自行记录。
pub async fn auth_middleware_with_state(
    State(state): State<AppState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let expected = match state.config.load().admin_token.clone() {
        Some(t) if !t.is_empty() => t,
        _ => {
            // admin 未启用：整个路由应当不会被挂载；兜底拒绝以防止配置误用
            return unauthorized();
        }
    };

    let provided = query.token.or_else(|| {
        headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.to_string())
    });

    let matches = match &provided {
        Some(t) if t.len() == expected.len() => t.as_bytes().ct_eq(expected.as_bytes()).into(),
        _ => false,
    };

    if matches {
        // v1.5.0 成功登录不写 audit（太频繁），但 ban 计数器清零
        let ip = super::audit::client_ip_from_headers(&headers);
        if let Some(ref ip) = ip {
            state.admin_limiter.record_success(ip);
        }
        return next.run(request).await;
    }

    // 失败路径
    let ip = super::audit::client_ip_from_headers(&headers);
    let token_prefix = provided.as_deref().map(super::audit::token_prefix);

    if let Some(ref ip_str) = ip {
        if state.admin_limiter.is_banned(ip_str) {
            tracing::warn!(ip = %ip_str, "admin 登录被 ban 拒绝");
            super::audit::spawn_record(
                &state,
                token_prefix.clone(),
                super::audit::ACTION_LOGIN_FAIL,
                Some("banned".to_string()),
                ip.clone(),
                false,
                Some(r#"{"reason":"banned"}"#.to_string()),
            );
            return unauthorized_banned();
        }

        let (over_limit, failed_count) = state.admin_limiter.record_fail(ip_str);
        super::audit::spawn_record(
            &state,
            token_prefix,
            super::audit::ACTION_LOGIN_FAIL,
            None,
            ip.clone(),
            false,
            Some(format!(r#"{{"failed_count":{failed_count}}}"#)),
        );

        if over_limit {
            tracing::warn!(ip = %ip_str, failed_count, "admin 登录连续失败触发 ban");
            return unauthorized_banned();
        }
    } else {
        super::audit::spawn_record(
            &state,
            token_prefix,
            super::audit::ACTION_LOGIN_FAIL,
            None,
            None,
            false,
            None,
        );
    }

    unauthorized()
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({"error": "未授权，请提供正确的 admin token"})),
    )
        .into_response()
}

fn unauthorized_banned() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        axum::Json(serde_json::json!({
            "error": "登录失败次数过多，当前 IP 已被临时封禁"
        })),
    )
        .into_response()
}
