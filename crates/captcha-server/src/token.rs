use std::net::IpAddr;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine as _};
use captcha_core::crypto;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// v1.4.0：IP hash 长度（sha256 前 16 字节 = 128 bits）。
pub const IP_HASH_LEN: usize = 16;

/// v1.4.0：UA hash 长度（sha256 前 8 字节 = 64 bits）。
pub const UA_HASH_LEN: usize = 8;

/// captcha_token 的荷载，用于业务后端 /siteverify 时还原信息。
///
/// v1.4.0 新增 `ip_hash` / `ua_hash`：当 site 开启 `bind_token_to_ip` /
/// `bind_token_to_ua` 时，token 发放时填入，siteverify 时强制比对。
/// 两字段均 opt-in，未开启时不进 payload，保持 token 紧凑且向后兼容。
#[derive(Debug, Serialize, Deserialize)]
struct Payload {
    challenge_id: String,
    site_key: String,
    exp: u64,

    /// SHA-256(client_ip)[0..16]，base64url。绑定 IP 时填入。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ip_hash: Option<String>,

    /// SHA-256(user_agent)[0..8]，base64url。绑定 UA 时填入。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ua_hash: Option<String>,
}

/// token 解析 + 校验后的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedToken {
    pub challenge_id: String,
    pub site_key: String,
    pub exp: u64,
    pub ip_hash: Option<[u8; IP_HASH_LEN]>,
    pub ua_hash: Option<[u8; UA_HASH_LEN]>,
}

/// 对客户端 IP 做规范化 hash：`sha256(ip.to_string())[0..16]`。
/// `IpAddr::to_string` 会输出规范化形式（IPv4 去前导零、IPv6 小写压缩），
/// 保证 verify 阶段与 siteverify 阶段 hash 一致。
pub fn hash_ip(ip: &IpAddr) -> [u8; IP_HASH_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(ip.to_string().as_bytes());
    let full = hasher.finalize();
    let mut out = [0u8; IP_HASH_LEN];
    out.copy_from_slice(&full[..IP_HASH_LEN]);
    out
}

/// 对 User-Agent 做 hash：`sha256(ua)[0..8]`。原串不做规范化，
/// 意味着业务后端传入的 `user_agent` 必须与浏览器 `/verify` 时发送的完全一致。
pub fn hash_ua(ua: &str) -> [u8; UA_HASH_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(ua.as_bytes());
    let full = hasher.finalize();
    let mut out = [0u8; UA_HASH_LEN];
    out.copy_from_slice(&full[..UA_HASH_LEN]);
    out
}

/// 常数时间比较两个 IP hash。
pub fn ip_hash_eq(a: &[u8; IP_HASH_LEN], b: &[u8; IP_HASH_LEN]) -> bool {
    a.ct_eq(b).into()
}

/// 常数时间比较两个 UA hash。
pub fn ua_hash_eq(a: &[u8; UA_HASH_LEN], b: &[u8; UA_HASH_LEN]) -> bool {
    a.ct_eq(b).into()
}

/// 生成 captcha_token。
///
/// 格式：`base64url(payload_json).base64url(sig)`。
/// `ip_hash` / `ua_hash` 为 `Some(..)` 时写入 payload；否则字段被 skip_serializing。
pub fn generate(
    challenge_id: &str,
    site_key: &str,
    ttl_secs: u64,
    secret: &[u8],
    ip_hash: Option<[u8; IP_HASH_LEN]>,
    ua_hash: Option<[u8; UA_HASH_LEN]>,
) -> (String, u64) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let exp = now_ms + ttl_secs * 1000;

    let payload = Payload {
        challenge_id: challenge_id.to_string(),
        site_key: site_key.to_string(),
        exp,
        ip_hash: ip_hash.map(|h| B64.encode(h)),
        ua_hash: ua_hash.map(|h| B64.encode(h)),
    };
    let payload_json = serde_json::to_vec(&payload).unwrap_or_default();
    let sig = crypto::sign(&payload_json, secret);

    let token = format!("{}.{}", B64.encode(&payload_json), B64.encode(sig));
    (token, exp)
}

/// 校验 captcha_token 签名 + 过期，返回完整 payload。
///
/// `secrets` 支持多把密钥（用于 v1.5.0 双 key 轮换场景）：
/// 任一把匹配即签名通过。典型场景下传入 `&cfg.verify_secrets()`。
pub fn verify_full(token: &str, secrets: &[&[u8]]) -> Option<VerifiedToken> {
    let (payload_b64, sig_b64) = token.split_once('.')?;

    let payload_bytes = B64.decode(payload_b64).ok()?;
    let sig_bytes = B64.decode(sig_b64).ok()?;
    let sig_arr: [u8; 32] = sig_bytes.as_slice().try_into().ok()?;

    if !crypto::verify_sig_any(&payload_bytes, &sig_arr, secrets) {
        return None;
    }

    let payload: Payload = serde_json::from_slice(&payload_bytes).ok()?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    if now_ms > payload.exp {
        return None;
    }

    let ip_hash = match payload.ip_hash {
        Some(s) => {
            let bytes = B64.decode(&s).ok()?;
            if bytes.len() != IP_HASH_LEN {
                return None;
            }
            let mut arr = [0u8; IP_HASH_LEN];
            arr.copy_from_slice(&bytes);
            Some(arr)
        }
        None => None,
    };

    let ua_hash = match payload.ua_hash {
        Some(s) => {
            let bytes = B64.decode(&s).ok()?;
            if bytes.len() != UA_HASH_LEN {
                return None;
            }
            let mut arr = [0u8; UA_HASH_LEN];
            arr.copy_from_slice(&bytes);
            Some(arr)
        }
        None => None,
    };

    Some(VerifiedToken {
        challenge_id: payload.challenge_id,
        site_key: payload.site_key,
        exp: payload.exp,
        ip_hash,
        ua_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_roundtrip_without_binding() {
        let secret = b"test-secret-key-long-enough-32-bytes!";
        let (token, _exp) = generate("cid-1", "pk_test", 300, secret, None, None);
        let result = verify_full(&token, &[secret]).unwrap();
        assert_eq!(result.challenge_id, "cid-1");
        assert_eq!(result.site_key, "pk_test");
        assert!(result.ip_hash.is_none());
        assert!(result.ua_hash.is_none());
    }

    #[test]
    fn token_roundtrip_with_ip_binding() {
        let secret = b"test-secret-key-long-enough-32-bytes!";
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let ih = hash_ip(&ip);
        let (token, _) = generate("cid-1", "pk_test", 300, secret, Some(ih), None);
        let result = verify_full(&token, &[secret]).unwrap();
        assert_eq!(result.ip_hash, Some(ih));
        assert!(result.ua_hash.is_none());
    }

    #[test]
    fn token_roundtrip_with_ua_binding() {
        let secret = b"test-secret-key-long-enough-32-bytes!";
        let uh = hash_ua("Mozilla/5.0 (Test)");
        let (token, _) = generate("cid-1", "pk_test", 300, secret, None, Some(uh));
        let result = verify_full(&token, &[secret]).unwrap();
        assert_eq!(result.ua_hash, Some(uh));
    }

    #[test]
    fn wrong_secret_rejects() {
        let (token, _) = generate("cid-1", "pk_test", 300, b"secret-a", None, None);
        assert!(verify_full(&token, &[b"secret-b"]).is_none());
    }

    #[test]
    fn tampered_token_rejects() {
        let secret = b"my-test-secret-key-long-enough!!";
        let (token, _) = generate("cid-1", "pk_test", 300, secret, None, None);
        let mut bad = token.clone();
        bad.push('X');
        assert!(verify_full(&bad, &[secret]).is_none());
    }

    #[test]
    fn expired_token_rejects() {
        let secret = b"test-secret-key-long-enough-32-b!";
        let (token, _) = generate("cid-1", "pk_test", 0, secret, None, None);
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(verify_full(&token, &[secret]).is_none());
    }

    #[test]
    fn ip_hash_deterministic() {
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        assert_eq!(hash_ip(&ip), hash_ip(&ip));
    }

    #[test]
    fn ip_hash_different_for_different_ips() {
        let a: IpAddr = "1.1.1.1".parse().unwrap();
        let b: IpAddr = "2.2.2.2".parse().unwrap();
        assert_ne!(hash_ip(&a), hash_ip(&b));
    }

    #[test]
    fn ipv6_canonicalization_consistent() {
        let a: IpAddr = "2001:db8::1".parse().unwrap();
        let b: IpAddr = "2001:0DB8:0000:0000:0000:0000:0000:0001".parse().unwrap();
        assert_eq!(hash_ip(&a), hash_ip(&b), "IpAddr::to_string 应规范化 IPv6");
    }

    #[test]
    fn ua_hash_deterministic() {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X)";
        assert_eq!(hash_ua(ua), hash_ua(ua));
    }

    #[test]
    fn ua_hash_different_for_different_strings() {
        assert_ne!(hash_ua("UA-A"), hash_ua("UA-B"));
    }

    #[test]
    fn tampered_ip_hash_length_rejected() {
        // 构造一个合法签名但 ip_hash 字段长度错误的 token
        let secret = b"test-secret-key-long-enough-32-b!";
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let payload = Payload {
            challenge_id: "cid".into(),
            site_key: "pk".into(),
            exp: now_ms + 300_000,
            ip_hash: Some(B64.encode([1u8, 2u8, 3u8])), // 3 字节，非法
            ua_hash: None,
        };
        let payload_json = serde_json::to_vec(&payload).unwrap();
        let sig = crypto::sign(&payload_json, secret);
        let token = format!("{}.{}", B64.encode(&payload_json), B64.encode(sig));
        assert!(verify_full(&token, &[secret]).is_none());
    }

    #[test]
    fn legacy_token_without_hash_fields_still_parses() {
        // 模拟 v1.3.x 旧 token（payload 无 ip_hash / ua_hash 字段）
        let secret = b"test-secret-key-long-enough-32-b!";
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let legacy_payload = serde_json::json!({
            "challenge_id": "cid-legacy",
            "site_key": "pk_test",
            "exp": now_ms + 300_000,
        });
        let payload_json = serde_json::to_vec(&legacy_payload).unwrap();
        let sig = crypto::sign(&payload_json, secret);
        let token = format!("{}.{}", B64.encode(&payload_json), B64.encode(sig));
        let result = verify_full(&token, &[secret]).unwrap();
        assert_eq!(result.challenge_id, "cid-legacy");
        assert!(result.ip_hash.is_none());
        assert!(result.ua_hash.is_none());
    }

    // v1.5.0 双 key 轮换
    #[test]
    fn verify_full_accepts_current_or_previous_secret() {
        let current: &[u8] = b"current-secret-key-32-bytes!!!!!!";
        let previous: &[u8] = b"previous-secret-key-32-bytes!!!!!";

        // 用 previous 签发（模拟轮换前发出的 token）
        let (old_token, _) = generate("cid-old", "pk_test", 300, previous, None, None);
        // 轮换后服务端 verify_secrets = [current, previous]
        assert!(verify_full(&old_token, &[current, previous]).is_some());
        // 若仅剩 current（完成轮换窗口）则拒绝
        assert!(verify_full(&old_token, &[current]).is_none());
    }
}
