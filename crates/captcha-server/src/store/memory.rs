use std::time::{SystemTime, UNIX_EPOCH};

use dashmap::DashMap;

/// 内存防重放存储。key = challenge.id，value = 过期时间戳（unix ms）。
/// 后台任务定期清理过期条目，避免内存无限增长。
pub struct MemoryStore {
    used: DashMap<String, u64>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            used: DashMap::new(),
        }
    }

    /// 原子地将挑战标记为已使用。
    /// 返回 `true` 表示首次使用成功；`false` 表示已被使用过（重放尝试）。
    pub fn mark_used(&self, id: &str, exp_ms: u64) -> bool {
        self.used.insert(id.to_string(), exp_ms).is_none()
    }

    /// 清理所有已过期条目，由后台 tokio 任务定期调用。
    pub fn cleanup_expired(&self) -> usize {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let before = self.used.len();
        self.used.retain(|_, exp| *exp > now_ms);
        before.saturating_sub(self.used.len())
    }

    pub fn len(&self) -> usize {
        self.used.len()
    }

    pub fn is_empty(&self) -> bool {
        self.used.is_empty()
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_used_once() {
        let s = MemoryStore::new();
        assert!(s.mark_used("a", u64::MAX));
        assert!(!s.mark_used("a", u64::MAX));
    }

    #[test]
    fn cleanup_removes_expired() {
        let s = MemoryStore::new();
        s.mark_used("expired", 0);
        s.mark_used("valid", u64::MAX);
        let removed = s.cleanup_expired();
        assert_eq!(removed, 1);
        assert_eq!(s.len(), 1);
    }
}
