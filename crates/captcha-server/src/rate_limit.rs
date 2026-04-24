//! 基于 IP 的限流中间件。
//! 使用 `governor` 提供的令牌桶算法 + DashMap 按 IP 维护限流状态。

use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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

// ──────────────── v1.5.0 admin 登录限流 + 失败 ban ────────────────

/// admin 失败 ban 阈值：窗口内累计失败次数达到此值触发 ban。
pub const ADMIN_FAIL_BAN_THRESHOLD: u32 = 30;
/// admin 失败计数窗口：超过此窗口的累计自动重置。
pub const ADMIN_FAIL_WINDOW_MS: u64 = 60 * 60 * 1000; // 1 小时
/// admin ban 持续时长。
pub const ADMIN_BAN_DURATION_MS: u64 = 15 * 60 * 1000; // 15 分钟

#[derive(Debug, Default, Clone)]
struct FailState {
    count: u32,
    first_fail_ms: u64,
    ban_until_ms: u64,
}

/// admin 登录限流器：按 IP 追踪失败次数，连续失败触发临时 ban。
#[derive(Default)]
pub struct AdminLoginLimiter {
    states: DashMap<String, FailState>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl AdminLoginLimiter {
    /// 检查 IP 是否处于 ban 中。
    pub fn is_banned(&self, ip: &str) -> bool {
        let now = now_ms();
        if let Some(entry) = self.states.get(ip) {
            return entry.ban_until_ms > now;
        }
        false
    }

    /// 记录一次登录失败，返回 `(是否触发新 ban, 当前失败累计)`。
    /// 自动执行窗口过期重置。
    pub fn record_fail(&self, ip: &str) -> (bool, u32) {
        let now = now_ms();
        let mut entry = self.states.entry(ip.to_string()).or_default();

        // 窗口过期：重置计数
        if now.saturating_sub(entry.first_fail_ms) > ADMIN_FAIL_WINDOW_MS {
            entry.count = 0;
            entry.first_fail_ms = now;
            entry.ban_until_ms = 0;
        }

        if entry.count == 0 {
            entry.first_fail_ms = now;
        }
        entry.count = entry.count.saturating_add(1);

        let triggered = if entry.count >= ADMIN_FAIL_BAN_THRESHOLD && entry.ban_until_ms <= now {
            entry.ban_until_ms = now + ADMIN_BAN_DURATION_MS;
            true
        } else {
            false
        };

        (triggered, entry.count)
    }

    /// 登录成功时清空失败计数（但保留已生效的 ban — 直到自然到期）。
    pub fn record_success(&self, ip: &str) {
        if let Some(mut entry) = self.states.get_mut(ip) {
            entry.count = 0;
            entry.first_fail_ms = 0;
        }
    }

    /// 管理接口用：返回当前所有 ban 中 IP 的列表。
    pub fn banned_ips(&self) -> Vec<(String, u64)> {
        let now = now_ms();
        self.states
            .iter()
            .filter(|e| e.ban_until_ms > now)
            .map(|e| (e.key().clone(), e.ban_until_ms))
            .collect()
    }

    /// 定期清理过期且无 ban 的条目，防止 DashMap 无限增长。
    pub fn cleanup(&self) {
        let now = now_ms();
        self.states.retain(|_, st| {
            st.ban_until_ms > now
                || now.saturating_sub(st.first_fail_ms) <= ADMIN_FAIL_WINDOW_MS
        });
    }
}

// ──────────────── v1.5.0 admin 通用 5/min 限流 ────────────────

/// admin /admin/api/* 的 5/min 速率限流。
/// 与业务 IpRateLimiter 独立，阈值更严格。
pub fn admin_rate_limiter() -> IpRateLimiter {
    IpRateLimiter::new(1, 5)
}
