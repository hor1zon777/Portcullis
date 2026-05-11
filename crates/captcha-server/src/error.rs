use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use std::fmt;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub success: bool,
    pub error: String,
}

/// 统一错误类型，映射到 HTTP 状态码。
#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    Conflict(String),
    Internal(String),
}

impl AppError {
    /// 取出对外可见的错误消息（保持与 HTTP 响应体一致）。
    pub fn message(&self) -> &str {
        match self {
            Self::BadRequest(m) | Self::Unauthorized(m) | Self::Conflict(m) | Self::Internal(m) => {
                m
            }
        }
    }
}

impl fmt::Display for AppError {
    /// 仅暴露 user-facing 文案，避免把 enum variant 名等内部细节泄露给客户端。
    /// 主要用于 `verify_batch` 内嵌结果中代替历史 `format!("{e:?}")` 的写法。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            Self::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m),
            Self::Conflict(m) => (StatusCode::CONFLICT, m),
            Self::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        let body = ErrorBody {
            success: false,
            error: msg,
        };
        (status, axum::Json(body)).into_response()
    }
}
