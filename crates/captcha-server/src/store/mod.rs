//! 防重放存储的抽象接口。
//!
//! v0.5.0 提供 `MemoryStore`（单机内存）。
//! v0.5.1 计划提供 `RedisStore`（多实例共享），实现同一 trait 即可插拔。
//!
//! 目前 handler 层直接使用 `MemoryStore` 的具体类型以避免 dyn dispatch 开销；
//! 未来切换 Redis 时改为 `Arc<dyn Store + Send + Sync>` 或 feature flag。

pub mod memory;

use crate::store::memory::StoreMetrics;

/// 防重放存储抽象。
///
/// 所有方法都是同步的。Redis 后端实现时可在内部使用 tokio blocking 或切换为 async trait。
pub trait Store: Send + Sync {
    /// 标记 challenge 已使用，返回 true 表示首次使用。
    fn mark_challenge_used(&self, id: &str, exp_ms: u64) -> bool;

    /// 标记 token 已核验，返回 true 表示首次使用。
    fn mark_token_used(&self, challenge_id: &str, exp_ms: u64) -> bool;

    /// 清理所有过期条目，返回清理数量。
    fn cleanup_expired(&self) -> usize;

    /// 当前条目数。
    fn len(&self) -> usize;

    /// 是否为空。
    fn is_empty(&self) -> bool;

    /// 导出指标供 Prometheus 采集。
    fn metrics(&self) -> StoreMetrics;
}

impl Store for memory::MemoryStore {
    fn mark_challenge_used(&self, id: &str, exp_ms: u64) -> bool {
        memory::MemoryStore::mark_challenge_used(self, id, exp_ms)
    }
    fn mark_token_used(&self, challenge_id: &str, exp_ms: u64) -> bool {
        memory::MemoryStore::mark_token_used(self, challenge_id, exp_ms)
    }
    fn cleanup_expired(&self) -> usize {
        memory::MemoryStore::cleanup_expired(self)
    }
    fn len(&self) -> usize {
        memory::MemoryStore::len(self)
    }
    fn is_empty(&self) -> bool {
        memory::MemoryStore::is_empty(self)
    }
    fn metrics(&self) -> StoreMetrics {
        memory::MemoryStore::metrics(self)
    }
}
