use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 旧版 Challenge（v1.2.x）不含 Argon2 参数时的默认回填值。
pub const LEGACY_M_COST: u32 = 4096;
pub const LEGACY_T_COST: u32 = 1;
pub const LEGACY_P_COST: u32 = 1;

/// v1.3.0 新默认值（OWASP 2024 推荐 Argon2id 第二档）。
pub const DEFAULT_M_COST: u32 = 19456;
pub const DEFAULT_T_COST: u32 = 2;
pub const DEFAULT_P_COST: u32 = 1;

fn default_m_cost() -> u32 {
    LEGACY_M_COST
}
fn default_t_cost() -> u32 {
    LEGACY_T_COST
}
fn default_p_cost() -> u32 {
    LEGACY_P_COST
}

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

    /// Argon2id memory cost (KiB)。v1.2.x 旧 JSON 无此字段时回填 4096。
    #[serde(default = "default_m_cost")]
    pub m_cost: u32,

    /// Argon2id time cost (iterations)。旧 JSON 无此字段时回填 1。
    #[serde(default = "default_t_cost")]
    pub t_cost: u32,

    /// Argon2id parallelism。旧 JSON 无此字段时回填 1。
    #[serde(default = "default_p_cost")]
    pub p_cost: u32,
}

impl Challenge {
    /// 生成用于 HMAC 签名的确定性字节表示。
    /// 格式：id_bytes | salt(16) | diff(1) | exp_le(8) | site_key_bytes | m_cost_le(4) | t_cost_le(4) | p_cost_le(4)
    pub fn to_sign_bytes(&self) -> Vec<u8> {
        let mut buf =
            Vec::with_capacity(self.id.len() + 16 + 1 + 8 + self.site_key.len() + 12);
        buf.extend_from_slice(self.id.as_bytes());
        buf.extend_from_slice(&self.salt);
        buf.push(self.diff);
        buf.extend_from_slice(&self.exp.to_le_bytes());
        buf.extend_from_slice(self.site_key.as_bytes());
        buf.extend_from_slice(&self.m_cost.to_le_bytes());
        buf.extend_from_slice(&self.t_cost.to_le_bytes());
        buf.extend_from_slice(&self.p_cost.to_le_bytes());
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
            m_cost: DEFAULT_M_COST,
            t_cost: DEFAULT_T_COST,
            p_cost: DEFAULT_P_COST,
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
        assert_eq!(ch.m_cost, ch2.m_cost);
        assert_eq!(ch.t_cost, ch2.t_cost);
        assert_eq!(ch.p_cost, ch2.p_cost);
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

    #[test]
    fn legacy_json_without_pow_params_fills_defaults() {
        let json = r#"{
            "id": "test-legacy",
            "salt": "AQIDBAUGBwgJCgsMDQ4PEA==",
            "diff": 18,
            "exp": 9999999999999,
            "site_key": "pk_old"
        }"#;
        let ch: Challenge = serde_json::from_str(json).unwrap();
        assert_eq!(ch.m_cost, LEGACY_M_COST);
        assert_eq!(ch.t_cost, LEGACY_T_COST);
        assert_eq!(ch.p_cost, LEGACY_P_COST);
    }

    #[test]
    fn sign_bytes_include_pow_params() {
        let ch1 = sample_challenge();
        let mut ch2 = sample_challenge();
        ch2.m_cost = 8192;
        assert_ne!(ch1.to_sign_bytes(), ch2.to_sign_bytes());

        ch2.m_cost = ch1.m_cost;
        ch2.t_cost = 3;
        assert_ne!(ch1.to_sign_bytes(), ch2.to_sign_bytes());
    }
}
