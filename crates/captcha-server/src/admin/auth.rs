use axum::extract::Query;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

#[derive(serde::Deserialize)]
pub struct TokenQuery {
    pub token: Option<String>,
}

pub async fn auth_middleware(
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let expected = request
        .extensions()
        .get::<String>()
        .cloned()
        .unwrap_or_default();

    let provided = query.token.or_else(|| {
        headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.to_string())
    });

    match provided {
        Some(t) if t == expected => next.run(request).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({"error": "未授权，请提供正确的 admin token"})),
        )
            .into_response(),
    }
}
