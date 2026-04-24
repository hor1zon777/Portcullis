use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// 使用 HMAC-SHA256 对数据签名。
pub fn sign(data: &[u8], secret: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC 接受任意长度密钥");
    mac.update(data);
    let result = mac.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result.into_bytes());
    out
}

/// 常数时间验证 HMAC 签名，防时序攻击。
pub fn verify_sig(data: &[u8], sig: &[u8; 32], secret: &[u8]) -> bool {
    let expected = sign(data, secret);
    expected.ct_eq(sig).into()
}

/// 尝试多把密钥验证签名，任一成功即算通过（用于 v1.5.0 双 key 轮换）。
///
/// 对每把 key 都跑完整的 `verify_sig`（内部为常数时间比较），
/// 因此不泄漏"哪把 key 命中"的侧信道。当 `secrets` 为空时返回 `false`。
pub fn verify_sig_any(data: &[u8], sig: &[u8; 32], secrets: &[&[u8]]) -> bool {
    let mut ok = false;
    for s in secrets {
        // 使用 `|` 而非 `||`，避免短路使不同 key 的总耗时差异被观察到。
        ok |= verify_sig(data, sig, s);
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_roundtrip() {
        let secret = b"test-secret-key-must-be-long-enough!!";
        let data = b"hello world";
        let sig = sign(data, secret);
        assert!(verify_sig(data, &sig, secret));
    }

    #[test]
    fn wrong_secret_rejects() {
        let data = b"payload";
        let sig = sign(data, b"secret-a");
        assert!(!verify_sig(data, &sig, b"secret-b"));
    }

    #[test]
    fn tampered_data_rejects() {
        let secret = b"my-secret";
        let sig = sign(b"original", secret);
        assert!(!verify_sig(b"tampered", &sig, secret));
    }

    #[test]
    fn signature_is_32_bytes() {
        let sig = sign(b"data", b"key");
        assert_eq!(sig.len(), 32);
    }

    #[test]
    fn deterministic() {
        let sig1 = sign(b"data", b"key");
        let sig2 = sign(b"data", b"key");
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn verify_any_accepts_current() {
        let data = b"payload";
        let sig = sign(data, b"current-secret");
        assert!(verify_sig_any(
            data,
            &sig,
            &[b"current-secret", b"previous-secret"]
        ));
    }

    #[test]
    fn verify_any_accepts_previous() {
        let data = b"payload";
        let sig = sign(data, b"previous-secret");
        assert!(verify_sig_any(
            data,
            &sig,
            &[b"current-secret", b"previous-secret"]
        ));
    }

    #[test]
    fn verify_any_rejects_unknown() {
        let data = b"payload";
        let sig = sign(data, b"alien-key");
        assert!(!verify_sig_any(
            data,
            &sig,
            &[b"current-secret", b"previous-secret"]
        ));
    }

    #[test]
    fn verify_any_empty_slice_rejects() {
        let sig = sign(b"payload", b"key");
        assert!(!verify_sig_any(b"payload", &sig, &[]));
    }

    #[test]
    fn verify_any_single_key_equivalent_to_verify_sig() {
        let data = b"payload";
        let sig = sign(data, b"only-key");
        assert_eq!(
            verify_sig(data, &sig, b"only-key"),
            verify_sig_any(data, &sig, &[b"only-key"])
        );
    }
}
