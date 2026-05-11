use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::RwLock;

use crate::admin::request_log::RequestLog;
use crate::config::Config;
use crate::db::Db;
use crate::rate_limit::AdminLoginLimiter;
use crate::risk::RiskTracker;
use crate::store::memory::MemoryStore;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ArcSwap<Config>>,
    pub store: Arc<MemoryStore>,
    pub risk: Arc<RwLock<RiskTracker>>,
    pub request_log: Arc<RequestLog>,
    pub db: Db,
    /// v1.5.0：admin 登录限流 + IP ban（仅对 /admin/api/* 生效，独立于业务限流器）
    pub admin_limiter: Arc<AdminLoginLimiter>,
}

impl AppState {
    pub fn new(config: Config, db: Db) -> Self {
        crate::db::migrate(&db);
        // 历史 v1.5.0 曾在此处对 sites.secret_key 做 HMAC 化以增强抗内存 dump 风险。
        // 因要求 secret_key 在管理面板中可再次查看，此迁移已禁用：新建站点存明文，
        // 仅遗留的已 hashed 行（secret_key_hashed=true）保持不可恢复——它们仍可
        // 通过 siteverify 验证（routes/siteverify.rs 同时支持明文与 HMAC 两种比较）。
        let risk_cfg = config.risk.clone();
        Self {
            config: Arc::new(ArcSwap::from_pointee(config)),
            store: Arc::new(MemoryStore::new()),
            risk: Arc::new(RwLock::new(RiskTracker::new(risk_cfg))),
            request_log: Arc::new(RequestLog::new()),
            db,
            admin_limiter: Arc::new(AdminLoginLimiter::default()),
        }
    }

    pub async fn reload_config(&self, new_config: Config) {
        let risk_cfg = new_config.risk.clone();
        let mut merged = new_config;
        // manifest signing key 以 DB 为准，防止热重载 toml/env 时覆盖管理面板生成/撤销的结果
        merged.manifest_signing_key =
            crate::db::load_server_secret_32(&self.db, "manifest_signing_key")
                .or(merged.manifest_signing_key);

        // 先拿到 risk 的 write 锁更新 CIDR / 滑动窗口阈值，再 publish 新 config。
        // 顺序说明：handler 路径是「先 load config，再 read risk」——若反过来先 store config
        // 再 update risk，handler 可能拿到「新 config + 旧 risk」的组合，新加的黑名单看似没生效。
        // 现在 risk 先就绪，新 config publish 后任何后续读到的 risk 至少已是新配置；
        // 唯一残余窗口是「reload 进入前的 handler 还持有旧 config」——这种 in-flight 请求
        // 用旧 config + 等待 risk 锁后的新 risk，影响仅限于"新黑名单可能晚一个请求生效"，
        // 不会导致已封禁 IP 被放行。
        let mut risk_guard = self.risk.write().await;
        risk_guard.update_config(risk_cfg);
        self.config.store(Arc::new(merged));
        drop(risk_guard);

        tracing::info!("配置已热重载");
    }
}
