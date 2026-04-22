use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// PoW 挑战结构。
/// 由服务端生成并签名，客户端解题后回传。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    /// 唯一标识（UUIDv4 字符串），用于防重放
    pub id: String,

    /// 16 字节随机盐（JSON 中 base64 编码）
    #[serde(serialize_with = "ser_salt", deserialize_with = "de_salt")]
    pub salt: [u8; 16],

    /// 要求的前导零比特数
    pub diff: u8,

    /// 过期时间戳（unix 毫秒）
    pub exp: u64,

    /// 调用方标识
    pub site_key: String,
}

impl Challenge {
    /// 生成用于 HMAC 签名的确定性字节表示。
    /// 格式：id_bytes | salt(16) | diff(1) | exp_le(8) | site_key_bytes
    pub fn to_sign_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.id.len() + 16 + 1 + 8 + self.site_key.len());
        buf.extend_from_slice(self.id.as_bytes());
        buf.extend_from_slice(&self.salt);
        buf.push(self.diff);
        buf.extend_from_slice(&self.exp.to_le_bytes());
        buf.extend_from_slice(self.site_key.as_bytes());
        buf
    }

    /// 检查挑战是否已过期。
    pub fn is_expired(&self) -> bool {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_millis() as u64;
        now_ms > self.exp
    }
}

fn ser_salt<S: Serializer>(salt: &[u8; 16], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&B64.encode(salt))
}

fn de_salt<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 16], D::Error> {
    let encoded = String::deserialize(d)?;
    let bytes = B64.decode(&encoded).map_err(serde::de::Error::custom)?;
    bytes
        .try_into()
        .map_err(|_| serde::de::Error::custom("salt 必须为 16 字节"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_challenge() -> Challenge {
        Challenge {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            salt: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            diff: 18,
            exp: u64::MAX,
            site_key: "pk_test".to_string(),
        }
    }

    #[test]
    fn json_roundtrip() {
        let ch = sample_challenge();
        let json = serde_json::to_string(&ch).unwrap();
        let ch2: Challenge = serde_json::from_str(&json).unwrap();
        assert_eq!(ch.id, ch2.id);
        assert_eq!(ch.salt, ch2.salt);
        assert_eq!(ch.diff, ch2.diff);
        assert_eq!(ch.exp, ch2.exp);
        assert_eq!(ch.site_key, ch2.site_key);
    }

    #[test]
    fn salt_is_base64_in_json() {
        let ch = sample_challenge();
        let json = serde_json::to_string(&ch).unwrap();
        assert!(json.contains(&B64.encode(ch.salt)));
    }

    #[test]
    fn sign_bytes_deterministic() {
        let ch = sample_challenge();
        assert_eq!(ch.to_sign_bytes(), ch.to_sign_bytes());
    }

    #[test]
    fn not_expired_with_max_exp() {
        let ch = sample_challenge();
        assert!(!ch.is_expired());
    }

    #[test]
    fn expired_with_zero_exp() {
        let mut ch = sample_challenge();
        ch.exp = 0;
        assert!(ch.is_expired());
    }
}
