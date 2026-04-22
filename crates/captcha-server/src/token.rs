use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine as _};
use captcha_core::crypto;
use serde::{Deserialize, Serialize};

/// captcha_token 的荷载，用于业务后端 /siteverify 时还原信息。
#[derive(Debug, Serialize, Deserialize)]
struct Payload {
    challenge_id: String,
    site_key: String,
    exp: u64,
}

/// 生成 captcha_token，格式：`base64url(payload_json).base64url(sig)`。
/// 业务后端通过 /siteverify 调用本服务进行校验。
pub fn generate(
    challenge_id: &str,
    site_key: &str,
    ttl_secs: u64,
    secret: &[u8],
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
    };
    let payload_json = serde_json::to_vec(&payload).unwrap_or_default();
    let sig = crypto::sign(&payload_json, secret);

    let token = format!("{}.{}", B64.encode(&payload_json), B64.encode(sig));
    (token, exp)
}

/// 校验 captcha_token。成功返回 `(challenge_id, site_key)`，失败返回 `None`。
pub fn verify(token: &str, secret: &[u8]) -> Option<(String, String)> {
    verify_with_exp(token, secret).map(|(cid, sk, _)| (cid, sk))
}

/// 校验 captcha_token，同时返回过期时间戳。
pub fn verify_with_exp(token: &str, secret: &[u8]) -> Option<(String, String, u64)> {
    let (payload_b64, sig_b64) = token.split_once('.')?;

    let payload_bytes = B64.decode(payload_b64).ok()?;
    let sig_bytes = B64.decode(sig_b64).ok()?;
    let sig_arr: [u8; 32] = sig_bytes.as_slice().try_into().ok()?;

    if !crypto::verify_sig(&payload_bytes, &sig_arr, secret) {
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

    Some((payload.challenge_id, payload.site_key, payload.exp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_roundtrip() {
        let secret = b"test-secret-key-long-enough-32-bytes!";
        let (token, _exp) = generate("cid-1", "pk_test", 300, secret);
        let result = verify(&token, secret);
        assert_eq!(result, Some(("cid-1".to_string(), "pk_test".to_string())));
    }

    #[test]
    fn wrong_secret_rejects() {
        let (token, _) = generate("cid-1", "pk_test", 300, b"secret-a");
        assert!(verify(&token, b"secret-b").is_none());
    }

    #[test]
    fn tampered_token_rejects() {
        let secret = b"my-test-secret-key-long-enough!!";
        let (token, _) = generate("cid-1", "pk_test", 300, secret);
        let mut bad = token.clone();
        bad.push('X');
        assert!(verify(&bad, secret).is_none());
    }

    #[test]
    fn expired_token_rejects() {
        let secret = b"test-secret-key-long-enough-32-b!";
        // ttl=0 立即过期
        let (token, _) = generate("cid-1", "pk_test", 0, secret);
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(verify(&token, secret).is_none());
    }
}
