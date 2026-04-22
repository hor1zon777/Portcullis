//! Prometheus 指标。通过 `metrics` crate 注册计数器 / 直方图。

use std::sync::Arc;
use std::time::Instant;

use axum::response::IntoResponse;
use metrics::{counter, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// 安装 Prometheus exporter，返回 handle 用于 `/metrics` 端点。
pub fn install() -> PrometheusHandle {
    PrometheusBuilder::new()
        .install_recorder()
        .expect("Prometheus recorder 安装失败")
}

/// GET /metrics
pub async fn metrics_handler(
    axum::extract::State(handle): axum::extract::State<PrometheusHandle>,
) -> impl IntoResponse {
    handle.render()
}

// ──────── 计数器名称常量 ────────
pub const CHALLENGE_ISSUED: &str = "captcha_challenge_issued_total";
pub const VERIFY_SUCCESS: &str = "captcha_verify_success_total";
pub const VERIFY_FAIL: &str = "captcha_verify_fail_total";
pub const SITEVERIFY_SUCCESS: &str = "captcha_siteverify_success_total";
pub const SITEVERIFY_FAIL: &str = "captcha_siteverify_fail_total";
pub const VERIFY_DURATION: &str = "captcha_verify_duration_seconds";

pub fn record_challenge_issued(site_key: &str) {
    counter!(CHALLENGE_ISSUED, "site_key" => site_key.to_string()).increment(1);
}

pub fn record_verify(site_key: &str, success: bool, started: Instant) {
    let label = if success { VERIFY_SUCCESS } else { VERIFY_FAIL };
    counter!(label, "site_key" => site_key.to_string()).increment(1);
    histogram!(VERIFY_DURATION, "site_key" => site_key.to_string())
        .record(started.elapsed().as_secs_f64());
}

pub fn record_siteverify(success: bool) {
    let label = if success {
        SITEVERIFY_SUCCESS
    } else {
        SITEVERIFY_FAIL
    };
    counter!(label).increment(1);
}

/// Store 指标（由 /metrics 时采集）。
pub fn register_store_metrics(store: &Arc<crate::store::memory::MemoryStore>) {
    let m = store.metrics();
    metrics::gauge!("captcha_store_challenges_used").set(m.challenges_used as f64);
    metrics::gauge!("captcha_store_tokens_used").set(m.tokens_used as f64);
}
