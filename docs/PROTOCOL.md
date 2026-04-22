# 协议规范

## 概览

PoW CAPTCHA 协议由三条 HTTP 端点构成：

| 端点 | 调用方 | 作用 |
|------|--------|------|
| `POST /api/v1/challenge` | 浏览器 SDK | 申请挑战 |
| `POST /api/v1/verify`    | 浏览器 SDK | 提交解答换取 token |
| `POST /api/v1/siteverify`| 业务后端   | 核验 token 有效性 |

全链路交互见 [README.md](../README.md#目录结构) 中的 ASCII 流程图。

## 算法参数

**双阶段设计**：

1. **Phase 1（一次性）**：`base_hash = Argon2id(password=challenge.id, salt=challenge.salt)`
   - Argon2id v0x13, m=4096 KiB, t=1, p=1, out=32 B
   - 内存硬化门槛，每个 challenge 必须付出 4 MiB 内存成本，抗 GPU
2. **Phase 2（迭代）**：`hash = SHA-256(base_hash || nonce_le_8B)`
   - 求解循环只跑 SHA-256，单次 ~3μs（WASM）
   - 难度判定：`leading_zero_bits(hash) >= challenge.diff`

**HMAC 签名**：HMAC-SHA256, 常数时间比较。

**服务端验证开销**：一次 Argon2id（~50ms）+ 一次 SHA-256（<1μs），O(1)。

> 算法常量定义在 [`captcha-core/src/pow.rs`](../crates/captcha-core/src/pow.rs)（`M_COST / T_COST / P_COST / ARGON2_OUT`）；任何变化都会导致历史 token 全部失效，必须同步推进客户端 WASM 与服务端版本。

## `POST /api/v1/challenge`

### 请求体

```json
{ "site_key": "pk_test" }
```

### 响应体

```json
{
  "success": true,
  "challenge": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "salt": "AQIDBAUGBwgJCgsMDQ4PEA==",
    "diff": 18,
    "exp": 1737900000000,
    "site_key": "pk_test"
  },
  "sig": "<base64 32 字节>"
}
```

字段说明：

| 字段 | 类型 | 说明 |
|------|------|------|
| `challenge.id` | string | UUIDv4，用于防重放 |
| `challenge.salt` | string | 16 字节随机盐，base64 标准编码 |
| `challenge.diff` | u8 | 前导零比特数（通常 12~22） |
| `challenge.exp` | u64 | 过期时间戳（unix 毫秒） |
| `challenge.site_key` | string | 站点标识 |
| `sig` | string | HMAC-SHA256 签名，base64 标准编码 |

### 错误

| 状态码 | 原因 |
|--------|------|
| 400 | `site_key` 未在配置中登记 |
| 500 | 随机数生成失败 |

## 签名算法

服务端对挑战的确定性字节序列签名：

```
bytes = id_utf8 || salt(16B) || diff(1B) || exp_le(8B) || site_key_utf8
sig   = HMAC-SHA256(server_secret, bytes)
```

客户端提交 verify 时原封不动回传 `challenge` 与 `sig`，服务端按同样规则重算并常数时间比较。

## `POST /api/v1/verify`

### 请求体

```json
{
  "challenge": { "id": "...", "salt": "...", "diff": 18, "exp": 1737900000000, "site_key": "pk_test" },
  "sig": "<base64>",
  "nonce": 184726
}
```

### 服务端校验顺序

1. HMAC 签名
2. 未过期（`now_ms <= exp`）
3. 未被重放（`challenge.id` 首次出现）
4. PoW 满足（`leading_zero_bits(argon2id(nonce, salt)) >= diff`）

### 响应体

```json
{
  "success": true,
  "captcha_token": "<base64url>.<base64url>",
  "exp": 1737900300000
}
```

### 错误

| 状态码 | 原因 |
|--------|------|
| 400 | 挑战已过期 / PoW 不满足 / sig 格式错 |
| 401 | 签名验证失败 |
| 409 | 挑战已被使用（重放） |

## `captcha_token` 结构

```
token = base64url(payload_json) + "." + base64url(sig_32B)
payload = { challenge_id, site_key, exp }
sig     = HMAC-SHA256(server_secret, payload_json)
```

与 JWT 类似但格式更紧凑：业务后端无需解析 token，仅通过 `/siteverify` 反向核验。

## `POST /api/v1/siteverify`

### 请求体

```json
{
  "token": "<captcha_token>",
  "secret_key": "sk_test_secret"
}
```

### 响应体

- 成功：
  ```json
  { "success": true, "challenge_id": "…", "site_key": "pk_test" }
  ```
- 失败：
  ```json
  { "success": false, "error": "token 无效或已过期" }
  ```

设计上始终返回 200 OK，用 `success` 字段表示校验结果，便于业务后端统一处理。

## 版本与兼容

v1 协议锁定于：
- Argon2 参数（见上）
- HMAC 输出长度（32 B）
- 签名字节序列格式

算法参数调整需同时推进客户端 WASM 与服务端版本号并做灰度；旧 token 一经算法变动即全部失效。
