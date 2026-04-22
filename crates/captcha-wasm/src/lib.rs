//! 浏览器端 PoW 求解内核（WASM）。
//! 与 captcha-server 共享 captcha-core 的 Argon2id + SHA-256 双阶段算法。

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use captcha_core::{challenge::Challenge, difficulty::leading_zero_bits, pow};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Debug, Deserialize)]
struct ChallengePayload {
    challenge: Challenge,
    sig: String,
}

#[derive(Debug, Serialize)]
struct SolveResult {
    nonce: u64,
    hash: String,
    attempts: u64,
    elapsed_ms: f64,
}

#[wasm_bindgen(start)]
pub fn init() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

// ────────────────── chunked solve API（主线程用） ──────────────────

/// 持久化求解状态。JS 侧通过 `create_solver()` 创建，`step()` 推进。
#[wasm_bindgen]
pub struct Solver {
    sig: String,
    challenge: Challenge,
    base_hash: [u8; 32],
    next_nonce: u64,
    diff: u32,
    started_at: f64,
    hard_limit: u64,
}

#[wasm_bindgen]
impl Solver {
    /// 推进求解。每次最多迭代 `chunk_size` 次，然后交还控制权。
    ///
    /// 返回 JS 对象 `{ found, nonce?, hash?, attempts, elapsed_ms?, exhausted }`
    pub fn step(&mut self, chunk_size: u64) -> JsValue {
        let end = (self.next_nonce + chunk_size).min(self.hard_limit);

        for nonce in self.next_nonce..end {
            let hash = pow::compute_pow_hash(&self.base_hash, nonce);
            if leading_zero_bits(&hash) >= self.diff {
                let elapsed = js_sys::Date::now() - self.started_at;
                let result = serde_wasm_bindgen::to_value(&StepResult {
                    found: true,
                    nonce: Some(nonce),
                    hash: Some(B64.encode(hash)),
                    attempts: nonce + 1,
                    elapsed_ms: Some(elapsed),
                    exhausted: false,
                })
                .unwrap();
                return result;
            }
        }

        self.next_nonce = end;
        let exhausted = self.next_nonce >= self.hard_limit;
        serde_wasm_bindgen::to_value(&StepResult {
            found: false,
            nonce: None,
            hash: None,
            attempts: self.next_nonce,
            elapsed_ms: None,
            exhausted,
        })
        .unwrap()
    }

    /// 获取挑战（JSON 序列化），用于提交 /verify。
    pub fn challenge_json(&self) -> String {
        serde_json::to_string(&self.challenge).unwrap()
    }

    /// 获取签名。
    pub fn sig(&self) -> String {
        self.sig.clone()
    }
}

#[derive(Serialize)]
struct StepResult {
    found: bool,
    nonce: Option<u64>,
    hash: Option<String>,
    attempts: u64,
    elapsed_ms: Option<f64>,
    exhausted: bool,
}

/// 创建 Solver 实例。
/// 一次性完成 Argon2 base hash 计算（~100ms），后续 `step()` 只跑 SHA-256。
#[wasm_bindgen]
pub fn create_solver(payload_json: &str, hard_limit: u64) -> Result<Solver, JsError> {
    let payload: ChallengePayload = serde_json::from_str(payload_json)
        .map_err(|e| JsError::new(&format!("payload JSON 解析失败: {e}")))?;

    let sig_bytes = B64
        .decode(&payload.sig)
        .map_err(|e| JsError::new(&format!("sig base64 解析失败: {e}")))?;
    if sig_bytes.len() != 32 {
        return Err(JsError::new("sig 长度必须为 32 字节"));
    }

    let now_ms = js_sys::Date::now() as u64;
    if now_ms > payload.challenge.exp {
        return Err(JsError::new("挑战已过期，请重新获取"));
    }

    let base_hash = pow::compute_base_hash(&payload.challenge);
    let diff = payload.challenge.diff as u32;

    Ok(Solver {
        sig: payload.sig,
        challenge: payload.challenge,
        base_hash,
        next_nonce: 0,
        diff,
        started_at: js_sys::Date::now(),
        hard_limit,
    })
}

// ────────────────── legacy 同步 API（Worker 用） ──────────────────

#[wasm_bindgen]
pub fn solve(
    payload_json: &str,
    max_iters: u64,
    progress_cb: &js_sys::Function,
    report_interval: u64,
) -> Result<JsValue, JsError> {
    let payload: ChallengePayload = serde_json::from_str(payload_json)
        .map_err(|e| JsError::new(&format!("payload JSON 解析失败: {e}")))?;

    let sig_bytes = B64
        .decode(&payload.sig)
        .map_err(|e| JsError::new(&format!("sig base64 解析失败: {e}")))?;
    if sig_bytes.len() != 32 {
        return Err(JsError::new("sig 长度必须为 32 字节"));
    }
    let now_ms = js_sys::Date::now() as u64;
    if now_ms > payload.challenge.exp {
        return Err(JsError::new("挑战已过期，请重新获取"));
    }

    let start = js_sys::Date::now();
    let this = JsValue::NULL;

    let result = pow::solve(&payload.challenge, max_iters, report_interval, |tries| {
        let _ = progress_cb.call1(&this, &JsValue::from_f64(tries as f64));
    });

    match result {
        Some((nonce, hash)) => {
            let elapsed_ms = js_sys::Date::now() - start;
            let out = SolveResult {
                nonce,
                hash: B64.encode(hash),
                attempts: nonce + 1,
                elapsed_ms,
            };
            serde_wasm_bindgen::to_value(&out)
                .map_err(|e| JsError::new(&format!("结果序列化失败: {e}")))
        }
        None => Err(JsError::new(
            "超出最大迭代次数仍未解出，请提升 max_iters 或降低难度",
        )),
    }
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
