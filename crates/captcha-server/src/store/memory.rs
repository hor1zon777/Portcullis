use std::time::{SystemTime, UNIX_EPOCH};

use dashmap::DashMap;

/// 内存防重放存储。
/// - `challenges_used`: challenge.id → exp，/verify 成功后写入
/// - `tokens_used`: challenge_id → exp，/siteverify 首次成功后写入
pub struct MemoryStore {
    challenges_used: DashMap<String, u64>,
    tokens_used: DashMap<String, u64>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            challenges_used: DashMap::new(),
            tokens_used: DashMap::new(),
        }
    }

    /// 标记 challenge 已使用（/verify 调用）。
    pub fn mark_challenge_used(&self, id: &str, exp_ms: u64) -> bool {
        self.challenges_used.insert(id.to_string(), exp_ms).is_none()
    }

    /// 标记 token 已核验（/siteverify 调用）。
    pub fn mark_token_used(&self, challenge_id: &str, exp_ms: u64) -> bool {
        self.tokens_used
            .insert(challenge_id.to_string(), exp_ms)
            .is_none()
    }

    pub fn cleanup_expired(&self) -> usize {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let before_c = self.challenges_used.len();
        self.challenges_used.retain(|_, exp| *exp > now_ms);
        let before_t = self.tokens_used.len();
        self.tokens_used.retain(|_, exp| *exp > now_ms);

        (before_c - self.challenges_used.len()) + (before_t - self.tokens_used.len())
    }

    pub fn len(&self) -> usize {
        self.challenges_used.len() + self.tokens_used.len()
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
}
