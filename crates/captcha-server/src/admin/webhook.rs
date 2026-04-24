//! admin 关键操作 webhook（v1.5.0）。
//!
//! 可选把 webhook URL 配置到 `[server].admin_webhook_url` 或
//! `CAPTCHA_ADMIN_WEBHOOK_URL`。关键操作发生时 POST JSON（Slack Incoming
//! Webhook 兼容格式）到该 URL。
//!
//! 失败 fire-and-forget，**不影响主流程**。内部使用 `reqwest`（rustls-tls）
//! 2 秒连接超时 + 3 秒总超时。

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Client;
use serde_json::json;

static CLIENT: OnceLock<Client> = OnceLock::new();

fn client() -> &'static Client {
    CLIENT.get_or_init(|| {
        Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(3))
            .user_agent(concat!("portcullis-captcha/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client 初始化失败")
    })
}

/// 异步投递一条 webhook 消息，不阻塞 handler。
pub fn spawn_post(
    url: String,
    action: &'static str,
    target: Option<String>,
    ip: Option<String>,
    success: bool,
    meta_json: Option<String>,
) {
    tokio::spawn(async move {
        let text = format!(
            "[Portcullis] admin action: {action} {} (ip={}, target={})",
            if success { "success" } else { "FAIL" },
            ip.as_deref().unwrap_or("-"),
            target.as_deref().unwrap_or("-"),
        );
        let meta_val = meta_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
        let payload = json!({
            "text": text,
            "action": action,
            "target": target,
            "ip": ip,
            "success": success,
            "meta": meta_val,
            "ts_ms": now_ms(),
        });

        match client().post(&url).json(&payload).send().await {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    tracing::warn!(url = %url, status = %status, action, "admin webhook 非 2xx 响应");
                } else {
                    tracing::debug!(url = %url, status = %status, action, "admin webhook 投递成功");
                }
            }
            Err(e) => {
                tracing::warn!(url = %url, action, error = %e, "admin webhook 投递失败");
            }
        }
    });
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
