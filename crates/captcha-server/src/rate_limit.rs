//! 基于 IP 的限流中间件。
//! 使用 `governor` 提供的令牌桶算法 + DashMap 按 IP 维护限流状态。

use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;

use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};

type IpLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// 每 IP 一个独立令牌桶。
#[derive(Clone)]
pub struct IpRateLimiter {
    limiters: Arc<DashMap<IpAddr, Arc<IpLimiter>>>,
    quota: Quota,
}

impl IpRateLimiter {
    pub fn new(per_second: u32, burst: u32) -> Self {
        let quota = Quota::per_second(NonZeroU32::new(per_second).expect("per_second > 0"))
            .allow_burst(NonZeroU32::new(burst).expect("burst > 0"));
        Self {
            limiters: Arc::new(DashMap::new()),
            quota,
        }
    }

    fn check(&self, ip: IpAddr) -> bool {
        // 容量保护：超过 50K IP 时清理（令牌桶满的条目可安全移除）
        if self.limiters.len() > 50_000 {
            let before = self.limiters.len();
            self.limiters.retain(|_, limiter| limiter.check().is_err());
            tracing::debug!("限流器清理：{} → {}", before, self.limiters.len());
        }
        let limiter = self
            .limiters
            .entry(ip)
            .or_insert_with(|| Arc::new(RateLimiter::direct(self.quota)))
            .clone();
        limiter.check().is_ok()
    }
}

/// 提取客户端 IP：优先 X-Forwarded-For 第一段，回落到 ConnectInfo。
pub fn extract_ip(headers: &HeaderMap, conn: Option<&SocketAddr>) -> Option<IpAddr> {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next() {
            if let Ok(ip) = first.trim().parse() {
                return Some(ip);
            }
        }
    }
    if let Some(real) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        if let Ok(ip) = real.parse() {
            return Some(ip);
        }
    }
    conn.map(|s| s.ip())
}

/// axum 中间件：超出限流返回 429。
pub async fn rate_limit_middleware(
    axum::extract::State(limiter): axum::extract::State<IpRateLimiter>,
    conn_info: Option<ConnectInfo<SocketAddr>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let ip = extract_ip(request.headers(), conn_info.as_ref().map(|ci| &ci.0));

    if let Some(ip) = ip {
        if !limiter.check(ip) {
            tracing::warn!(client_ip = %ip, "限流：拒绝请求");
            return (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(serde_json::json!({
                    "success": false,
                    "error": "请求过于频繁，请稍后再试"
                })),
            )
                .into_response();
        }
    }

    next.run(request).await
}
