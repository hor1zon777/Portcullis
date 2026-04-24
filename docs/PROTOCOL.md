# 协议规范

## 概览

PoW CAPTCHA 协议由三条 HTTP 端点构成：

| 端点 | 调用方 | 作用 |
|------|--------|------|
| `POST /api/v1/challenge` | 浏览器 SDK | 申请挑战 |
| `POST /api/v1/verify`    | 浏览器 SDK | 提交解答换取 token |
| `POST /api/v1/siteverify`| 业务后端   | 核验 token 有效性 |

全链路交互见 [README.md](../README.md#工作原理) 中的 ASCII 流程图。

## 算法参数（v1.3+ 每站点可配）

**双阶段设计**：

1. **Phase 1（一次性）**：`base_hash = Argon2id(password=challenge.id, salt=challenge.salt, params=challenge.m_cost/t_cost/p_cost)`
   - Argon2id v0x13, 参数由 challenge 下发，默认 **m=19456 KiB, t=2, p=1, out=32 B**（OWASP 2024 推荐）
   - 内存硬化门槛，每个 challenge 必须付出 `m_cost` KiB 内存成本，抗 GPU
   - v1.2.x 及之前硬编码 4 MiB；v1.3.0 起参数由 challenge 携带并经 HMAC 签名保护
2. **Phase 2（迭代）**：`hash = SHA-256(base_hash || nonce_le_8B)`
   - 求解循环只跑 SHA-256，单次 ~3μs（WASM）
   - 难度判定：`leading_zero_bits(hash) >= challenge.diff`

**HMAC 签名**：HMAC-SHA256, 常数时间比较。v1.5+ 支持 `current` + `previous` 双 key 平滑轮换：签发永远用 current，验证对两者都尝试。

**服务端验证开销**：一次 Argon2id（默认 ~20ms）+ 一次 SHA-256（<1μs），O(1)。

**参数范围校验**：`m_cost ∈ [8, 65536]`（KiB）、`t_cost ∈ [1, 10]`、`p_cost` 固定为 1。超范围服务端返回 400。

> 算法实现在 [`captcha-core/src/pow.rs`](../crates/captcha-core/src/pow.rs)；常量（`LEGACY_* / DEFAULT_*`）在 [`captcha-core/src/challenge.rs`](../crates/captcha-core/src/challenge.rs)。

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
    "site_key": "pk_test",
    "m_cost": 19456,
    "t_cost": 2,
    "p_cost": 1
  },
  "sig": "<base64 32 字节>"
}
```

字段说明：

| 字段 | 类型 | 自 | 说明 |
|------|------|----|------|
| `challenge.id` | string | v1.0 | UUIDv4，用于防重放 |
| `challenge.salt` | string | v1.0 | 16 字节随机盐，base64 标准编码 |
| `challenge.diff` | u8 | v1.0 | 前导零比特数（通常 12~22） |
| `challenge.exp` | u64 | v1.0 | 过期时间戳（unix 毫秒） |
| `challenge.site_key` | string | v1.0 | 站点标识 |
| `challenge.m_cost` | u32 | v1.3 | Argon2 memory cost (KiB)；旧客户端 JSON 无此字段时 `serde(default)` 回填 4096 |
| `challenge.t_cost` | u32 | v1.3 | Argon2 time cost；旧 JSON 回填 1 |
| `challenge.p_cost` | u32 | v1.3 | Argon2 parallelism；固定 1 |
| `sig` | string | v1.0 | HMAC-SHA256 签名，base64 标准编码 |

### 错误

| 状态码 | 原因 |
|--------|------|
| 400 | `site_key` 未在配置中登记 / IP 绑定启用但无法识别真实 IP（v1.4+） |
| 401 | 来源 Origin 不在站点白名单 |
| 500 | 随机数生成失败 |

## 签名算法

服务端对挑战的确定性字节序列签名（v1.3+）：

```
bytes = id_utf8 || salt(16B) || diff(1B) || exp_le(8B) || site_key_utf8
     || m_cost_le(4B) || t_cost_le(4B) || p_cost_le(4B)
sig   = HMAC-SHA256(server_secret, bytes)
```

客户端提交 verify 时原封不动回传 `challenge` 与 `sig`，服务端按同样规则重算并常数时间比较。**任何字段篡改（含 Argon2 参数）都会导致签名失败**。

v1.5+ 启用 `CAPTCHA_SECRET_PREVIOUS` 时，服务端依次尝试 `current` 与 `previous` 计算签名，任一匹配即通过——遍历每把 key 都跑完整 HMAC（使用 `|=` 累积结果）以避免时序侧信道。

## `POST /api/v1/verify`

### 请求体

```json
{
  "challenge": {
    "id": "...", "salt": "...", "diff": 18, "exp": 1737900000000,
    "site_key": "pk_test", "m_cost": 19456, "t_cost": 2, "p_cost": 1
  },
  "sig": "<base64>",
  "nonce": 184726
}
```

客户端原样回传 challenge 结构。Argon2 参数一并回传，服务端据此重算验证。

### 服务端校验顺序

1. Origin 白名单（若站点配置了 `origins`）
2. HMAC 签名（v1.5+ 双 key 尝试）
3. 未过期（`now_ms <= exp`）
4. 未被重放（`challenge.id` 首次出现，memory store + SQLite 双写）
5. PoW 满足（`leading_zero_bits(Argon2id(id, salt, m/t/p) || SHA256(base || nonce_le)) >= diff`）
6. **v1.4+ 身份绑定**：如站点开启 `bind_token_to_ip`，提取 XFF/XRI 算 `hash_ip`；`bind_token_to_ua` 同理。hash 写入 token payload

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
| 400 | 挑战已过期 / PoW 不满足 / sig 格式错 / IP 绑定启用但无法识别真实 IP |
| 401 | 签名验证失败 / Origin 不在白名单 |
| 409 | 挑战已被使用（重放） |

## `captcha_token` 结构

```
token = base64url(payload_json) + "." + base64url(sig_32B)
payload = { challenge_id, site_key, exp, ip_hash?, ua_hash? }
sig     = HMAC-SHA256(current_server_secret, payload_json)
```

Payload 字段：

| 字段 | 自 | 说明 |
|------|----|------|
| `challenge_id` | v1.0 | 原 challenge 的 UUID，用于单次使用去重 |
| `site_key` | v1.0 | 站点标识 |
| `exp` | v1.0 | token 过期时间戳（unix ms） |
| `ip_hash` | v1.4 | 可选；`sha256(client_ip.to_string())[..16]` 的 base64url；仅当站点启用 `bind_token_to_ip` 时存在 |
| `ua_hash` | v1.4 | 可选；`sha256(user_agent)[..8]` 的 base64url；仅当站点启用 `bind_token_to_ua` 时存在 |

`ip_hash` / `ua_hash` 标注 `#[serde(default, skip_serializing_if = "Option::is_none")]`：未启用绑定时不进 payload，保持紧凑 + 兼容 v1.3.x。

与 JWT 类似但格式更紧凑：业务后端无需解析 token，仅通过 `/siteverify` 反向核验。

## `POST /api/v1/siteverify`

### 请求体

```json
{
  "token": "<captcha_token>",
  "secret_key": "sk_test_secret_at_least16",
  "client_ip": "203.0.113.5",
  "user_agent": "Mozilla/5.0 ..."
}
```

| 字段 | 自 | 必填 | 说明 |
|------|----|------|------|
| `token` | v1.0 | 是 | `/verify` 返回的 captcha_token |
| `secret_key` | v1.0 | 是 | 站点明文 secret_key（**v1.5+ 服务端 HMAC 后比对**） |
| `client_ip` | v1.4 | 条件 | 启用 `bind_token_to_ip` 的 site 必填，否则返回失败 |
| `user_agent` | v1.4 | 条件 | 启用 `bind_token_to_ua` 的 site 必填 |

### 服务端校验顺序

1. token 签名 + 过期（v1.5+ 双 key 尝试）
2. site 存在性
3. `secret_key` 比较：v1.4.x 及之前是常数时间明文比较；**v1.5+ 对明文做 HMAC 后再与存储的 hash 比较**（支持 master 双 key 轮换）
4. token 自身的 ip_hash / ua_hash 若存在，强制与请求的 `client_ip` / `user_agent` 匹配；**token 无 hash 时忽略这两个字段**（兼容未开绑定的 site）
5. 单次使用（`challenge_id` 首次核验）

### 响应体

- 成功：
  ```json
  { "success": true, "challenge_id": "…", "site_key": "pk_test" }
  ```
- 失败（始终 200 OK，便于统一处理）：
  ```json
  { "success": false, "error": "token 无效或已过期" }
  ```

常见 `error`：`token 无效或已过期` / `site_key 已下线` / `secret_key 不匹配` / `token 已被核验过（单次使用）` / `token 要求 IP 绑定，但 siteverify 未携带 client_ip` / `client_ip 与 token 绑定不一致` / `client_ip 不是合法的 IP 地址` / `token 要求 UA 绑定，但 siteverify 未携带 user_agent`。

## 版本与兼容

`/api/v1/*` 协议自 v1.0.0 冻结：
- **新增字段向后兼容**：challenge 加 `m/t/p_cost`（v1.3）、siteverify 加 `client_ip/user_agent`（v1.4）都不影响老客户端
- **删除/改名字段会走 `/api/v2/*`**，v1 至少保留 12 个月
- 算法参数（Argon2 / HMAC 输出长度 / 签名字节序列）仍锁定；v1.3+ 起**参数由 challenge 动态下发**，运行期可按站点调整

历史行为变化：
- v1.3：Argon2 参数硬编码 → challenge 下发 + 签名覆盖
- v1.4：Token 可选携带 ip_hash / ua_hash
- v1.5：服务端 `secret_key` 改 HMAC 存储（不影响业务方后端 API）、支持 `CAPTCHA_SECRET` 双 key 轮换
