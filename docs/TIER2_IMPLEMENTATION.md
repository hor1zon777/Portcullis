# CAPTCHA SDK 加固 Tier 2 实施进度

> 前置：Tier 1 已上线（`docs/TIER1_IMPLEMENTATION.md`），`/sdk/manifest.json` 发布版本 + SRI integrity。
> 目标：为 manifest 增加 Ed25519 签名，抵御中间人替换 manifest（TLS 被绕过或终止在攻击者处的场景）。
> 启动时间：2026-04-23

---

## 威胁模型与方案取舍

### Tier 1 已防御

- **SDK 字节被篡改** → 浏览器 `<script integrity=...>` 拒加载
- **SDK 通道被降级** → HTTPS + HSTS 保障传输完整性

### Tier 2 补位的剩余威胁

- **Manifest 本身被篡改**：攻击者若能修改 `/sdk/manifest.json` 响应（例如：运营商代理、中间代理 CDN 配置错误、受损的跨域反代），可以把 `integrity` 改成匹配恶意 SDK 的哈希，SRI 就被绕过。
- **弱点**：Portcullis 服务本身被攻陷的场景下，私钥同样泄露，签名无意义。所以 Tier 2 只抵御"传输路径被篡改"，不抵御"源被攻陷"。

### 不做的

- **不做双密钥轮换**：本次只支持单 signing key。轮换流程通过"主站先接受新公钥 → Portcullis 切换私钥"的两步部署完成，文档说明。
- **不做 PKI / 证书链**：Ed25519 裸密钥配发，通过带外渠道（管理面板导出 → 管理员复制粘贴到主站配置）。

---

## 协议设计

### 密钥形态

- **私钥 seed**：Ed25519 32 字节，base64 编码后放 env `CAPTCHA_MANIFEST_SIGNING_KEY` 或 toml `[server].manifest_signing_key`
- **公钥**：32 字节，从 seed 派生；通过管理面板 API `/admin/api/manifest-pubkey` 导出给主站管理员
- **CLI 工具**：`captcha-server gen-manifest-key` 生成一对，stdout 分行输出 seed / 公钥（base64）

### 签名位置

- **放 response header** `X-Portcullis-Signature: <base64(64-byte sig)>`
- **不放 JSON body**：避免客户端做"规范化 JSON"才能验签的复杂性。主站拿 response 原始 bytes + header 即可 verify。

### 签名对象

- response body 的原始 bytes（`serde_json::to_vec(&manifest)` 的输出）
- 主站：`fetch → await r.text() → verify(bytes(text), sig, pubkey) → JSON.parse(text)`

### 向后兼容

- 未配置 signing key 时：manifest 不带 `X-Portcullis-Signature` header，主站拿到 response 后检测缺失 → 降级为 Tier 1 行为（只用 integrity，不验签）
- 已配置 signing key 时：manifest 一定带 header；主站验签失败应 reject（不能静默降级，否则攻击者去掉 header 即可绕过）

### 主站侧期望行为

```typescript
const resp = await fetch(`${endpoint}/sdk/manifest.json`);
const body = await resp.text();
const sig = resp.headers.get('X-Portcullis-Signature');

if (MANIFEST_PUBKEY_CONFIGURED) {
  if (!sig) throw new Error('Portcullis 已配置签名但响应缺 header');
  if (!verify(sig, body, MANIFEST_PUBKEY)) throw new Error('manifest 签名不匹配');
}

const manifest = JSON.parse(body);
// ... 按 Tier 1 流程加载 SDK with SRI
```

---

## 变更文件清单（计划）

| 文件 | 变更 | 说明 |
|---|---|---|
| `crates/captcha-server/Cargo.toml` | 加依赖 | `ed25519-dalek = "2"` |
| `crates/captcha-server/src/config.rs` | 新增字段 | `manifest_signing_key: Option<[u8;32]>`；env + toml 解析；`Commands::GenManifestKey` |
| `crates/captcha-server/src/main.rs` | 分支 | 处理 `GenManifestKey` 子命令 |
| `crates/captcha-server/src/static_assets.rs` | 改造 | `serve_sdk` 接 `State<AppState>`；`render_manifest` 带可选签名 |
| `crates/captcha-server/src/lib.rs` | 路由 | `/sdk/*file` 路由绑定 state |
| `crates/captcha-server/src/admin/handlers.rs` | 新增 handler | `GET /admin/api/manifest-pubkey` |
| `crates/captcha-server/src/admin/mod.rs` | 新增路由 | 注册上面 handler |
| `crates/captcha-server/tests/integration.rs` | 追加测试 | 签名存在性、签名验证、admin 端点 |

---

## 端点清单

| 方法/路径 | 鉴权 | 响应 |
|---|---|---|
| `GET /sdk/manifest.json` | 无 | 200 + JSON；配置了 signing key 时带 `X-Portcullis-Signature` 响应头 |
| `GET /admin/api/manifest-pubkey` | admin token | `{enabled: bool, pubkey?: string(base64)}` |

CLI:

```
$ captcha-server gen-manifest-key
私钥 seed (保密，写入 CAPTCHA_MANIFEST_SIGNING_KEY):
  <base64-32bytes>

公钥 (公开，配置到主站):
  <base64-32bytes>
```

---

## 进度状态

- [x] T2.0 写进度文档初版
- [x] T2.1 加 ed25519-dalek 依赖
- [x] T2.2 Config 加 manifest_signing_key 字段与加载
- [x] T2.3 CLI `gen-manifest-key` 子命令
- [x] T2.4 static_assets.rs 签名 manifest + `serve_sdk` 接 State
- [x] T2.5 admin API `/admin/api/manifest-pubkey`
- [x] T2.6 单元 + 集成测试
- [x] T2.7 更新文档与完成日志

---

## 完成日志

**完成时间：** 2026-04-23

### 文件变更

| 文件 | 变更 | 说明 |
|---|---|---|
| `crates/captcha-server/Cargo.toml` | 加依赖 | `ed25519-dalek = { version = "2", default-features = false, features = ["std", "fast"] }` |
| `crates/captcha-server/src/config.rs` | 加字段 + 子命令枚举 + 解析函数 | `manifest_signing_key: Option<[u8;32]>`；`Commands::GenManifestKey`；`parse_signing_key()`；`print_gen_manifest_key()`；`print_config_template()` 增补注释 |
| `crates/captcha-server/src/main.rs` | 分支 | 处理 `GenManifestKey` |
| `crates/captcha-server/src/static_assets.rs` | handler 改造 | `serve_sdk` 加 `State<AppState>` 提取器；`render_manifest` 接受 `Option<&SigningKey>`；签 body 字节 → `X-Portcullis-Signature` 响应头 |
| `crates/captcha-server/src/admin/handlers.rs` | 新增 handler | `manifest_pubkey` 返回 `{enabled, pubkey?, algorithm}` |
| `crates/captcha-server/src/admin/mod.rs` | 注册路由 | `GET /admin/api/manifest-pubkey` |
| `crates/captcha-server/tests/integration.rs` | 追加 5 条测试 + 补 `manifest_signing_key: None` 字段 | 见"测试覆盖" |

### 端点清单（Tier 2 新增 / 扩展）

| 方法/路径 | 鉴权 | 行为 |
|---|---|---|
| `GET /sdk/manifest.json` | 无 | 未配置签名密钥时：与 Tier 1 一致。配置时：追加响应头 `X-Portcullis-Signature: <base64(64-byte Ed25519 sig)>`，签对象为 response body 原始字节 |
| `GET /admin/api/manifest-pubkey` | admin token | `{enabled: bool, pubkey?: base64, algorithm: "ed25519"}` |
| CLI `captcha-server gen-manifest-key` | 本地 | stdout 输出私钥 seed + 公钥（均 base64） |

### 实现要点

1. **签名对象选择**：response body 的字节（`serde_json::to_vec(&Manifest)` 的输出）。主站 `await r.text()` + `verify(bytes(text), sig, pk)` 即可，无需规范化 JSON。
2. **签名承载位置**：HTTP 响应头而非 JSON 字段。好处：manifest body 保持干净 JSON；缺点：主站必须用原始 response 字节验证，不能 parse 后重新 serialize（这本来就是规范化 JSON 的难点——此方案绕开）。
3. **seed 存储形态**：`[u8; 32]` 而非 `SigningKey` 实例。理由：`SigningKey` 不 `Clone`，而 `Config` 要 `Clone`（`ArcSwap::load()` / 热重载）。每次 `render_manifest` 时 `SigningKey::from_bytes(&seed)` 开销为 ns 级派生常量一次，manifest 端点本身 5 分钟缓存 hit，可以接受。
4. **向后兼容保证**：未配 signing key 时不发 `X-Portcullis-Signature`，行为与 Tier 1 相同；已有 Tier 1 客户端无需改动。
5. **轮换策略**：当前只支持单 key。若轮换频次高再加"current + previous"双密钥设计；此前通过两步部署 + 主站侧过渡代码完成（见 `INTEGRATION.md` 方式 D+）。

### 测试覆盖

`static_assets` 单元测试追加：
- `ed25519_sign_verify_roundtrip`：验证 sign(body) → base64 → decode → Signature → verify 全链路，并验证 body 篡改后 verify 失败

`config` 单元测试新增 4 条：
- `parse_signing_key_roundtrip`
- `parse_signing_key_rejects_wrong_length`
- `parse_signing_key_rejects_invalid_base64`
- `parse_signing_key_trims_whitespace`

集成测试新增 5 条：
- `manifest_unsigned_when_key_absent` — 未配 key 时 manifest 无 `X-Portcullis-Signature`
- `manifest_signed_verifies_with_pubkey` — 配 key 后用同种子派生公钥验签成功；篡改 body 后验签失败
- `admin_manifest_pubkey_disabled` — 未配 key 时 admin API 返回 `{enabled: false}`
- `admin_manifest_pubkey_enabled_returns_matching_key` — 配 key 后 admin API 返回 base64 公钥，与 seed 派生出的公钥字节完全一致
- `admin_manifest_pubkey_requires_auth` — 无 Bearer Token 返回 401

### 测试结果

```
cargo test --workspace:
  captcha-core          23 passed
  captcha-server lib    29 passed (Tier 2 新增 4 + roundtrip 1 → 净增 5)
  integration           17 passed (Tier 2 新增 5)
cargo clippy -p captcha-server --all-targets    0 warnings
```

### 手动验证

```bash
# 生成密钥
$ captcha-server gen-manifest-key
私钥 seed (保密...):
  cXGcWeEMYwf9rn//+GRrp3ukC5WMtG8Ra9YMn3CEinA=
公钥 (公开...):
  l6WQs9St3VV5H9hPdh4yoBI5WxNnL+XIea2q/xwsNqE=

# 启动时配置
export CAPTCHA_MANIFEST_SIGNING_KEY=cXGcWeEMYwf9rn//+GRrp3ukC5WMtG8Ra9YMn3CEinA=
captcha-server --config captcha.toml

# 主站视角：
curl -si http://localhost:8787/sdk/manifest.json | head
# HTTP/1.1 200 OK
# content-type: application/json; charset=utf-8
# cache-control: public, max-age=300
# cross-origin-resource-policy: cross-origin
# x-portcullis-signature: <base64>
# ...

# 管理员视角：
curl -sH 'Authorization: Bearer <admin_token>' \
     http://localhost:8787/admin/api/manifest-pubkey | jq .
# {"enabled": true, "pubkey": "l6WQs9...", "algorithm": "ed25519"}
```

### 已知限制（文档化，不当缺陷）

1. **单 signing key**：不支持 current + previous 双密钥。轮换需两步部署（见 `INTEGRATION.md` 方式 D+）。
2. **公钥带外分发**：不提供公开读取公钥的端点，公钥只能通过 `/admin/api/manifest-pubkey`（需 admin token）导出，由管理员复制到主站配置。这是故意的安全选择，防止攻击者在篡改 manifest 的同时伪造匹配的公钥。
3. **服务被攻陷仍不可防**：Tier 2 只防"传输链路被篡改"。Portcullis 主机被攻陷场景下私钥泄露，签名对攻击者无门槛。这类场景需配合主机安全、入侵检测、私钥 HSM 化——不在本项目范围。

### 下一步

- [ ] 通知主站团队（M3u8Preview_Go）评估是否采用 Tier 2
- [ ] 如采用：主站构建期注入 `VITE_PORTCULLIS_PUBKEY`；可选地在 admin-ui 增加"显示公钥"页面
- [ ] 可选：admin-ui 加一个"查看/复制 manifest 公钥"按钮（本次只做 API，UI 另外排期）
- [ ] 发 Portcullis v1.1.3（Tier 1 + Tier 2 合并版；先集成测试环境验证后再打 tag）
