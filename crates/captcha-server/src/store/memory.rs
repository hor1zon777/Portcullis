use std::time::{SystemTime, UNIX_EPOCH};

use dashmap::DashMap;

const DEFAULT_MAX_ENTRIES: usize = 100_000;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 内存防重放存储。带容量上限，超出时强制清理过期条目。
pub struct MemoryStore {
    challenges_used: DashMap<String, u64>,
    tokens_used: DashMap<String, u64>,
    max_entries: usize,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_ENTRIES)
    }

    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            challenges_used: DashMap::new(),
            tokens_used: DashMap::new(),
            max_entries,
        }
    }

    pub fn mark_challenge_used(&self, id: &str, exp_ms: u64) -> bool {
        self.enforce_capacity();
        self.challenges_used.insert(id.to_string(), exp_ms).is_none()
    }

    pub fn mark_token_used(&self, challenge_id: &str, exp_ms: u64) -> bool {
        self.enforce_capacity();
        self.tokens_used
            .insert(challenge_id.to_string(), exp_ms)
            .is_none()
    }

    pub fn cleanup_expired(&self) -> usize {
        let now = now_ms();
        let before_c = self.challenges_used.len();
        self.challenges_used.retain(|_, exp| *exp > now);
        let before_t = self.tokens_used.len();
        self.tokens_used.retain(|_, exp| *exp > now);
        (before_c - self.challenges_used.len()) + (before_t - self.tokens_used.len())
    }

    pub fn len(&self) -> usize {
        self.challenges_used.len() + self.tokens_used.len()
    }

    pub fn is_empty(&self) -> bool {
        self.challenges_used.is_empty() && self.tokens_used.is_empty()
    }

    /// 总条目数超出上限时触发紧急清理。
    fn enforce_capacity(&self) {
        if self.len() >= self.max_entries {
            tracing::warn!(
                entries = self.len(),
                max = self.max_entries,
                "MemoryStore 接近容量上限，触发紧急清理"
            );
            self.cleanup_expired();
        }
    }

    /// 报告 metrics（供 Prometheus 采集）。
    pub fn metrics(&self) -> StoreMetrics {
        StoreMetrics {
            challenges_used: self.challenges_used.len(),
            tokens_used: self.tokens_used.len(),
            max_entries: self.max_entries,
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StoreMetrics {
    pub challenges_used: usize,
    pub tokens_used: usize,
    pub max_entries: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_replay_blocked() {
        let s = MemoryStore::new();
        assert!(s.mark_challenge_used("a", u64::MAX));
        assert!(!s.mark_challenge_used("a", u64::MAX));
    }

    #[test]
    fn token_single_use() {
        let s = MemoryStore::new();
        assert!(s.mark_token_used("cid-1", u64::MAX));
        assert!(!s.mark_token_used("cid-1", u64::MAX));
    }

    #[test]
    fn cleanup_removes_both() {
        let s = MemoryStore::new();
        s.mark_challenge_used("c-expired", 0);
        s.mark_challenge_used("c-valid", u64::MAX);
        s.mark_token_used("t-expired", 0);
        s.mark_token_used("t-valid", u64::MAX);
        let removed = s.cleanup_expired();
        assert_eq!(removed, 2);
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn capacity_triggers_cleanup() {
        let s = MemoryStore::with_capacity(5);
        // 插入 5 个已过期条目
        for i in 0..5 {
            s.mark_challenge_used(&format!("c-{i}"), 0);
        }
        assert_eq!(s.len(), 5);
        // 第 6 个触发 enforce_capacity，清掉所有过期条目
        s.mark_challenge_used("c-fresh", u64::MAX);
        assert_eq!(s.len(), 1);
    }
}
