//! v1.6.0：admin 路径后缀校验 middleware。
//!
//! 所有 admin API 形如 `/admin/{suffix}/api/...`。本 middleware 从 URL path
//! 中提取 `{suffix}`，与 `state.config.admin_path_suffix` 常数时间比较：
//! - 完全相同 → 放行
//! - 任何差异 → 直接返回 404（不暴露 admin 入口的存在）
//!
//! 这一层独立于 token 鉴权（auth.rs），目的是在 token 校验之前就把扫描器、
//! 引用了旧 URL 的脚本等流量挡掉；也避免暴力穷举 token 时连"管理端点存在"
//! 这一信息都不应该提供。

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::state::AppState;

/// 从 URL path 中取出 `/admin/{suffix}/...` 的 suffix 段。
/// 路径不符合形态时返回 None。
fn extract_path_suffix(path: &str) -> Option<&str> {
    // path 形如 "/admin/<suffix>/api/...", split 后 ["", "admin", "<suffix>", "api", ...]
    let mut iter = path.split('/');
    let _empty = iter.next()?; // ""
    let admin = iter.next()?;
    if admin != "admin" {
        return None;
    }
    iter.next()
}

/// 把任意输入压成 32 字节摘要，便于始终在等长缓冲上做 `ct_eq`，
/// 不因长度短路泄露 expected suffix 的长度。
fn fingerprint(input: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

pub async fn admin_path_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let expected = match state.config.load().admin_path_suffix.clone() {
        Some(s) if !s.is_empty() => s,
        _ => {
            // 没配 suffix —— 启动期理应已 seed 过；这里兜底拒绝以防误暴露
            tracing::error!("admin_path_suffix 未配置，拒绝访问 admin 路由");
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    let path = request.uri().path();
    let provided = extract_path_suffix(path).unwrap_or("");

    // 常数时间比较，避免按长度短路或按前缀差异短路。
    let provided_fp = fingerprint(provided.as_bytes());
    let expected_fp = fingerprint(expected.as_bytes());
    if !bool::from(provided_fp.ct_eq(&expected_fp)) {
        // 故意不留任何 admin 提示，返回纯 404
        return StatusCode::NOT_FOUND.into_response();
    }

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_admin_suffix_from_path() {
        assert_eq!(
            extract_path_suffix("/admin/abc1234567/api/stats"),
            Some("abc1234567")
        );
        assert_eq!(extract_path_suffix("/admin/xyz/api"), Some("xyz"));
        assert_eq!(extract_path_suffix("/admin/xyz"), Some("xyz"));
        assert_eq!(extract_path_suffix("/api/v1/challenge"), None);
        assert_eq!(extract_path_suffix("/admin"), None);
        assert_eq!(extract_path_suffix("/"), None);
        assert_eq!(extract_path_suffix(""), None);
    }
}
