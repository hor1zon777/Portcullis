use std::sync::Arc;

use crate::config::Config;
use crate::store::memory::MemoryStore;

/// 全局应用状态，axum handler 通过 `State<AppState>` 提取。
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub store: Arc<MemoryStore>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            store: Arc::new(MemoryStore::new()),
        }
    }
}
