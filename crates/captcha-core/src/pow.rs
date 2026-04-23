use argon2::{Algorithm, Argon2, Params, Version};
use sha2::{Digest, Sha256};

use crate::challenge::Challenge;
use crate::difficulty::leading_zero_bits;

// ────────────────────── 双阶段 PoW 算法 ──────────────────────
//
// Phase 1（一次性）：Argon2id(challenge.id, salt, m/t/p) → base_hash
//   保留内存硬化特性，GPU 每个 challenge 必须付出 m_cost KiB 内存成本。
//   参数从 challenge 结构中读取（v1.3.0+），不再硬编码。
//
// Phase 2（迭代）：SHA-256(base_hash || nonce_le) → pow_hash
//   纯计算型哈希，WASM 中每次 ~2-5μs，可在 1-2 秒内完成数十万次迭代。
//
// 服务端验证：重算一次 Argon2 + 一次 SHA-256，O(1)。
// ──────────────────────────────────────────────────────────────

const ARGON2_OUT: usize = 32;

fn build_argon2(m_cost: u32, t_cost: u32, p_cost: u32) -> Argon2<'static> {
    let params = Params::new(m_cost, t_cost, p_cost, Some(ARGON2_OUT))
        .expect("Argon2 参数无效");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Phase 1：计算 Argon2id base hash（每个 challenge 仅一次）。
/// 参数从 challenge.m_cost / t_cost / p_cost 读取。
pub fn compute_base_hash(challenge: &Challenge) -> [u8; ARGON2_OUT] {
    let argon2 = build_argon2(challenge.m_cost, challenge.t_cost, challenge.p_cost);
    let mut output = [0u8; ARGON2_OUT];
    argon2
        .hash_password_into(challenge.id.as_bytes(), &challenge.salt, &mut output)
        .expect("Argon2 哈希失败");
    output
}

/// Phase 2：SHA-256(base_hash || nonce_le_bytes)，迭代内循环。
pub fn compute_pow_hash(base: &[u8; 32], nonce: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(base);
    hasher.update(nonce.to_le_bytes());
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// 完整哈希（Argon2 + SHA-256），供服务端单次验证调用。
pub fn compute_full_hash(challenge: &Challenge, nonce: u64) -> [u8; 32] {
    let base = compute_base_hash(challenge);
    compute_pow_hash(&base, nonce)
}

/// 求解挑战（双阶段）。
///
/// 1. 一次 Argon2id 生成 base_hash（参数从 challenge 读取）
/// 2. 迭代 SHA-256 直到前导零 ≥ diff
pub fn solve<F>(
    challenge: &Challenge,
    max_iters: u64,
    report_interval: u64,
    mut progress_fn: F,
) -> Option<(u64, [u8; 32])>
where
    F: FnMut(u64),
{
    let target = challenge.diff as u32;
    let base = compute_base_hash(challenge);

    for nonce in 0..max_iters {
        if report_interval > 0 && nonce > 0 && nonce % report_interval == 0 {
            progress_fn(nonce);
        }
        let hash = compute_pow_hash(&base, nonce);
        if leading_zero_bits(&hash) >= target {
            return Some((nonce, hash));
        }
    }
    None
}

/// 验证解答：一次 Argon2 + 一次 SHA-256 + 前导零检查。
pub fn verify_solution(challenge: &Challenge, nonce: u64) -> bool {
    let hash = compute_full_hash(challenge, nonce);
    leading_zero_bits(&hash) >= challenge.diff as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::challenge::{LEGACY_M_COST, LEGACY_T_COST, LEGACY_P_COST, DEFAULT_M_COST, DEFAULT_T_COST, DEFAULT_P_COST};

    fn make_challenge(diff: u8) -> Challenge {
        Challenge {
            id: "test-id-001".to_string(),
            salt: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            diff,
            exp: u64::MAX,
            site_key: "test".to_string(),
            m_cost: LEGACY_M_COST,
            t_cost: LEGACY_T_COST,
            p_cost: LEGACY_P_COST,
        }
    }

    fn make_challenge_new_defaults(diff: u8) -> Challenge {
        Challenge {
            id: "test-id-001".to_string(),
            salt: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            diff,
            exp: u64::MAX,
            site_key: "test".to_string(),
            m_cost: DEFAULT_M_COST,
            t_cost: DEFAULT_T_COST,
            p_cost: DEFAULT_P_COST,
        }
    }

    #[test]
    fn solve_diff_0_instant() {
        let ch = make_challenge(0);
        let result = solve(&ch, 10, 0, |_| {});
        assert!(result.is_some());
        let (nonce, _) = result.unwrap();
        assert!(verify_solution(&ch, nonce));
    }

    #[test]
    fn solve_diff_8() {
        let ch = make_challenge(8);
        let result = solve(&ch, 1_000_000, 0, |_| {});
        assert!(result.is_some(), "diff=8 应在百万次内解出");
        let (nonce, _) = result.unwrap();
        assert!(verify_solution(&ch, nonce));
    }

    #[test]
    fn solve_diff_18_fast() {
        let ch = make_challenge(18);
        let result = solve(&ch, 10_000_000, 0, |_| {});
        assert!(result.is_some(), "diff=18 应在千万次内解出");
        let (nonce, _) = result.unwrap();
        assert!(verify_solution(&ch, nonce));
    }

    #[test]
    fn verify_rejects_bad_nonce() {
        let ch = make_challenge(20);
        let hash = compute_full_hash(&ch, 0);
        let zeros = leading_zero_bits(&hash);
        assert_eq!(verify_solution(&ch, 0), zeros >= 20);
    }

    #[test]
    fn progress_callback_fires() {
        let ch = make_challenge(30);
        let mut count = 0u64;
        let _ = solve(&ch, 64, 16, |_| count += 1);
        assert_eq!(count, 3);
    }

    #[test]
    fn base_hash_deterministic() {
        let ch = make_challenge(8);
        assert_eq!(compute_base_hash(&ch), compute_base_hash(&ch));
    }

    #[test]
    fn different_nonce_different_hash() {
        let ch = make_challenge(8);
        let base = compute_base_hash(&ch);
        assert_ne!(compute_pow_hash(&base, 0), compute_pow_hash(&base, 1));
    }

    #[test]
    fn different_challenge_different_base() {
        let ch1 = make_challenge(8);
        let mut ch2 = make_challenge(8);
        ch2.id = "test-id-002".to_string();
        assert_ne!(compute_base_hash(&ch1), compute_base_hash(&ch2));

        let mut ch3 = make_challenge(8);
        ch3.salt = [99u8; 16];
        assert_ne!(compute_base_hash(&ch1), compute_base_hash(&ch3));
    }

    #[test]
    fn different_params_different_base() {
        let ch1 = make_challenge(8);
        let mut ch2 = make_challenge(8);
        ch2.m_cost = 8192;
        assert_ne!(compute_base_hash(&ch1), compute_base_hash(&ch2));
    }

    #[test]
    fn solve_with_new_defaults() {
        let ch = make_challenge_new_defaults(8);
        let result = solve(&ch, 1_000_000, 0, |_| {});
        assert!(result.is_some(), "diff=8 @ 19456/2/1 应在百万次内解出");
        let (nonce, _) = result.unwrap();
        assert!(verify_solution(&ch, nonce));
    }
}
