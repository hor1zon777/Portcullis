//! v1.5.0：站点 `secret_key` hash 化存储工具。
//!
//! 原始 `secret_key` 是站点管理员和业务后端共享的凭证。v1.5 之前明文存 SQLite：
//! DB 泄漏即等于所有站点密钥泄漏。v1.5.0 起改存 `HMAC-SHA256(CAPTCHA_SECRET, secret_key)`
//! 的 base64，siteverify 时再对请求值做相同 HMAC 后常数时间比较。
//!
//! 迁移语义：
//! - `SiteConfig.secret_key_hashed = false` 表示内部存储的是明文（升级前遗留行
//!   或 env/toml 热重载中未处理）
//! - `true` 表示已 hash 化，安全存储
//!
//! 轮换 `CAPTCHA_SECRET` 时，旧的 hash 无法直接重新计算——需要管理员重新
//! 生成所有站点的 secret_key。这是一次性迁移的代价，换来 DB 泄漏不再等于
//! 密钥泄漏。

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use captcha_core::crypto;
use subtle::ConstantTimeEq;

/// `HMAC-SHA256(master_secret, plain_secret_key)` 的 base64 标准编码（44 字符）。
pub fn hash(plain: &str, master: &[u8]) -> String {
    B64.encode(crypto::sign(plain.as_bytes(), master))
}

/// 常数时间验证 `provided_plain` 的 HMAC 是否等于 `stored_hash`。
/// `stored_hash` 是 [`hash`] 的返回值。
pub fn verify(provided_plain: &str, stored_hash: &str, master: &[u8]) -> bool {
    let actual = hash(provided_plain, master);
    let a = actual.as_bytes();
    let b = stored_hash.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

/// 尝试多把 master secret 验证 `stored_hash`（用于 v1.5.0 CAPTCHA_SECRET 双 key 轮换）。
/// 任一 master 匹配即算通过。每把都跑完整 [`verify`]，时序侧信道等价。
pub fn verify_any(provided_plain: &str, stored_hash: &str, masters: &[&[u8]]) -> bool {
    let mut ok = false;
    for m in masters {
        ok |= verify(provided_plain, stored_hash, m);
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_base64_44_chars() {
        let h = hash("my-secret-key", b"master-secret-at-least-32-bytes!!");
        assert_eq!(h.len(), 44, "HMAC-SHA256 base64 应为 44 字符");
        assert!(B64.decode(&h).is_ok());
    }

    #[test]
    fn verify_accepts_correct_plain() {
        let master = b"master-secret-at-least-32-bytes!!";
        let h = hash("my-sk", master);
        assert!(verify("my-sk", &h, master));
    }

    #[test]
    fn verify_rejects_wrong_plain() {
        let master = b"master-secret-at-least-32-bytes!!";
        let h = hash("my-sk", master);
        assert!(!verify("other-sk", &h, master));
    }

    #[test]
    fn verify_rejects_wrong_master() {
        let h = hash("my-sk", b"master-a-at-least-32-bytes-long!!");
        assert!(!verify("my-sk", &h, b"master-b-at-least-32-bytes-long!!"));
    }

    #[test]
    fn verify_rejects_empty_and_tampered() {
        let master = b"master-secret-at-least-32-bytes!!";
        let h = hash("my-sk", master);
        assert!(!verify("", &h, master));
        assert!(!verify("my-sk", "", master));
        // 篡改 1 字符
        let mut bad = h.clone();
        bad.replace_range(0..1, "X");
        assert!(!verify("my-sk", &bad, master));
    }
}
