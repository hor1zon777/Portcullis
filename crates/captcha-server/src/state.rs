use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::risk::RiskTracker;
use crate::store::memory::MemoryStore;

/// 全局应用状态。
/// - `config`: ArcSwap 支持热重载（lock-free 读取）
/// - `risk`: RwLock 因 RiskTracker 需要可变 update_config
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ArcSwap<Config>>,
    pub store: Arc<MemoryStore>,
    pub risk: Arc<RwLock<RiskTracker>>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let risk_cfg = config.risk.clone();
        Self {
            config: Arc::new(ArcSwap::from_pointee(config)),
            store: Arc::new(MemoryStore::new()),
            risk: Arc::new(RwLock::new(RiskTracker::new(risk_cfg))),
        }
    }

    /// 热重载配置。
    pub async fn reload_config(&self, new_config: Config) {
        let risk_cfg = new_config.risk.clone();
        self.config.store(Arc::new(new_config));
        self.risk.write().await.update_config(risk_cfg);
        tracing::info!("配置已热重载");
    }
}
