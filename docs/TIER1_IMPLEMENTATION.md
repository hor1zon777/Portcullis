# CAPTCHA SDK 加固 Tier 1 实施进度

> 需求来源：`docs/CAPTCHA_SDK_HARDENING.md`（主站 M3u8Preview_Go 提出的配合建议）
> 目标：让 Portcullis SDK 可被 `<script integrity=...>` 校验，为主站 dynamic `captchaEndpoint` 场景提供运行时可验证的 SRI 清单。
> 启动时间：2026-04-23

---

## 目标与范围（Tier 1）

| 编号 | 能力 | 价值 | 成本 |
|---|---|---|---|
| T1.1 | `build.rs` 注入 `BUILD_TIMESTAMP` | manifest `builtAt` 字段 | 极小 |
| T1.2 | `/sdk/manifest.json` 端点 | 主站按清单加载并带 SRI | 小 |
| T1.3 | 版本化只读路径 `/sdk/v{version}/*file` | 浏览器长缓存 + 版本锚点 | 小 |
| T1.4 | SHA-384 `integrity` 计算（基于已有 rust-embed 字节） | SRI 核心素材 | 小 |
| T1.5 | `Cross-Origin-Resource-Policy: cross-origin` 头 | 配合主站启用 COEP 时必需 | 极小 |
| T1.6 | 保留 `/sdk/*file` 旧路径（向后兼容） | 已接入主站不至于因升级断流 | 0 |

**Tier 2（Ed25519 签名 manifest）不在本次范围**，等主站 Tier 1 对接稳定后再评估。

---

## 变更文件清单（计划）

| 文件 | 变更类型 | 说明 |
|---|---|---|
| `crates/captcha-server/build.rs` | 修改 | 新增 `BUILD_TIMESTAMP` 环境变量注入 |
| `crates/captcha-server/src/static_assets.rs` | 重构 | 新增 `serve_manifest`、版本化分派、SHA-384、CORP |
| `crates/captcha-server/src/lib.rs` | 无需改动 | 现有 `/sdk/*file` 单一路由即可覆盖新路径（handler 内 dispatch） |
| `crates/captcha-server/tests/integration.rs` | 追加 | 新增 manifest / 版本路径 / CORP 头测试 |

---

## manifest.json 协议

```
GET /sdk/manifest.json → application/json

{
  "version": "1.1.2",
  "builtAt": 1745400000,
  "artifacts": {
    "pow-captcha.js": {
      "url": "/sdk/v1.1.2/pow-captcha.js",
      "integrity": "sha384-<base64>",
      "size": 11284
    },
    "captcha_wasm.js":      { "url": "...", "integrity": "...", "size": ... },
    "captcha_wasm_bg.wasm": { "url": "...", "integrity": "...", "size": ... }
  }
}
```

响应头：
- `Content-Type: application/json; charset=utf-8`
- `Cache-Control: public, max-age=300`（主站 5 分钟拉一次足够）
- `Access-Control-Allow-Origin: *`
- `Cross-Origin-Resource-Policy: cross-origin`

---

## 路径策略

| 路径 | Cache-Control | ETag | Integrity 素材 |
|---|---|---|---|
| `/sdk/manifest.json` | `public, max-age=300` | 无 | 自身作为 SRI 清单 |
| `/sdk/v{SDK_VERSION}/pow-captcha.js` | `public, max-age=31536000, immutable` | ✅ | ✅ |
| `/sdk/v{SDK_VERSION}/captcha_wasm.js` | 同上 | ✅ | ✅ |
| `/sdk/v{SDK_VERSION}/captcha_wasm_bg.wasm` | 同上 | ✅ | ✅ |
| `/sdk/v{不匹配}/...` | — | — | 404 |
| `/sdk/pow-captcha.js`（旧路径） | `public, max-age=3600` | ✅ | — |

当 Portcullis 升级（例如 1.1.2 → 1.1.3）：
- 旧 `/sdk/v1.1.2/...` 整体 404（rust-embed 编译期绑定，旧字节已不在二进制里）
- 主站每次加载先拉 `manifest.json` → 自动切到 `/sdk/v1.1.3/...`
- 主站若短暂命中 stale manifest → `integrity` 校验失败 → fallback 到旧路径（不带 SRI）

这是已知代价。文档里明确告知即可。

---

## 进度状态

- [x] T1.0 写进度文档初版
- [x] T1.1 build.rs 注入 BUILD_TIMESTAMP
- [x] T1.2 重构 static_assets.rs（manifest + 版本路径 + SHA-384 + CORP）
- [x] T1.3 `cargo check` / `cargo test` 通过
- [x] T1.4 集成测试覆盖
- [x] T1.5 更新本文档完成日志

---

## 完成日志

**完成时间：** 2026-04-23
**版本：** 1.1.2（`CARGO_PKG_VERSION`）

### 文件变更

| 文件 | 变更 | 行为 |
|---|---|---|
| `crates/captcha-server/build.rs` | 修改 | `cargo:rustc-env=BUILD_TIMESTAMP=<epoch>` + `rerun-if-changed=build.rs` |
| `crates/captcha-server/src/static_assets.rs` | 重写 | 见"实现要点" |
| `crates/captcha-server/tests/integration.rs` | 追加 5 条测试 | 见"测试覆盖" |
| `crates/captcha-server/src/lib.rs` | 未改动 | 现有 `/sdk/*file` 单路由 + handler 内分派即可覆盖全部新路径 |

### 实现要点

1. **单路由 handler 分派**
   `serve_sdk` 按 `file` 前缀分派到三类处理：
   - `manifest.json` → `render_manifest()`
   - `v{X.Y.Z}/...` 且版本匹配 → `serve_asset(..., CACHE_IMMUTABLE)`
   - `v{X.Y.Z}/...` 版本不匹配且段形似版本号 → 404
   - 其它 → 旧路径 `serve_asset(..., CACHE_LEGACY)`（max-age=3600）

2. **哈希缓存 `META_CACHE`**
   `OnceLock<HashMap<String, AssetMeta>>`，`AssetMeta` 同时存 SHA-256 十六进制（ETag）与 `sha384-<base64>`（SRI integrity），首次请求时 lazy init。

3. **CORP 统一附加**
   所有资源 / 304 / manifest 响应一律带 `Cross-Origin-Resource-Policy: cross-origin`。

4. **`looks_like_version` 保守匹配**
   仅当段以 ASCII 数字开头、全部由 `[0-9A-Za-z.-]` 组成时才按版本路径判定，避免未来出现名字以 `v` 开头的真实文件（如 `video.js`）被误判。

### 端点清单

| 方法/路径 | 响应 |
|---|---|
| `GET /sdk/manifest.json` | JSON `{version, builtAt, artifacts}`；`Cache-Control: public, max-age=300`；CORP |
| `GET /sdk/v1.1.2/pow-captcha.js` | 200；`Cache-Control: public, max-age=31536000, immutable`；ETag；CORP |
| `GET /sdk/v1.1.2/captcha_wasm.js` | 同上 |
| `GET /sdk/v1.1.2/captcha_wasm_bg.wasm` | 同上 |
| `GET /sdk/v99.99.99/pow-captcha.js` | 404 |
| `GET /sdk/pow-captcha.js` | 200；`Cache-Control: public, max-age=3600`；ETag；CORP |
| `GET /sdk/does-not-exist.js` | 404 |

### 测试覆盖

新增 5 条集成测试（`tests/integration.rs`）：

| 测试名 | 验证项 |
|---|---|
| `sdk_manifest_json` | 200 / JSON 格式 / `version` / `builtAt` 数值 / `integrity` 以 `sha384-` 开头 / `url` 拼接正确 / CORP / `Cache-Control: max-age=300` |
| `sdk_versioned_path_current_version` | 200 / `Cache-Control: immutable` + `31536000` / ETag 存在 / CORP |
| `sdk_versioned_path_unknown_version_404` | 404 |
| `sdk_legacy_path_backward_compatible` | 200 / `Cache-Control: max-age=3600` / 不含 `immutable` |
| `sdk_unknown_file_404` | 404 |

静态资源产物缺失时测试自动降级跳过（`sdk_assets_available()` 守卫），CI 环境可单独构建后再跑。

`static_assets` 子模块 4 条单元测试覆盖 `SDK_VERSION`/`looks_like_version`/SRI 格式。

### 测试结果

```
running 23 tests  (captcha-core)                test result: ok. 23 passed
running 24 tests  (captcha-server lib)          test result: ok. 24 passed
running 12 tests  (captcha-server integration)  test result: ok. 12 passed
cargo clippy -p captcha-server --all-targets    0 warnings
```

### 接入测试（手动）

```bash
# 启动服务后
curl -s http://localhost:8787/sdk/manifest.json | jq .
# → {"version":"1.1.2","builtAt":...,"artifacts":{"pow-captcha.js":{"url":"/sdk/v1.1.2/pow-captcha.js","integrity":"sha384-...","size":...},...}}

curl -sI http://localhost:8787/sdk/v1.1.2/pow-captcha.js
# → Cache-Control: public, max-age=31536000, immutable
# → Cross-Origin-Resource-Policy: cross-origin
# → ETag: "<sha256hex>"

curl -sI http://localhost:8787/sdk/v99.99.99/pow-captcha.js
# → HTTP/1.1 404 Not Found
```

### 已知限制（文档化，不当缺陷）

1. **旧版本 404**：升级二进制（1.1.2 → 1.1.3）后，旧版本字节从 rust-embed 消失，`/sdk/v1.1.2/*` 整体 404。主站每次加载先拉 manifest → 自动切到新版本。短暂命中 stale manifest 的请求会因 `integrity` 不匹配被浏览器拒绝，此时主站需 fallback 到旧路径 `/sdk/pow-captcha.js`（无 SRI，降级）。
2. **BUILD_TIMESTAMP 粒度**：由 build.rs 在 Cargo 重新构建时刻写入，`rerun-if-changed` 限定仅 SDK 产物或 build.rs 变更才会刷新，代表的是"SDK 最后被重新嵌入的时间"而非每次 `cargo build` 时间，符合 manifest `builtAt` 的语义。
3. **manifest 本身无签名**：Tier 1 不做 Ed25519 签名。威胁模型依赖 HTTPS + HSTS 防止中间人替换 manifest。Tier 2 可选。

### 主站侧对接（M3u8Preview_Go）

Portcullis 已准备好。主站 `web/client/src/components/auth/CaptchaWidget.tsx` 改造示意：

```typescript
async function loadSdkWithSri(endpoint: string) {
  const ac = new AbortController();
  const t = setTimeout(() => ac.abort(), 3000);
  let manifest: any;
  try {
    manifest = await fetch(`${endpoint}/sdk/manifest.json`, {
      cache: 'no-store',
      signal: ac.signal,
    }).then((r) => r.json());
  } catch {
    // manifest 不可用 → 降级到旧路径（无 SRI）
    return injectScript(`${endpoint}/sdk/pow-captcha.js`);
  } finally {
    clearTimeout(t);
  }

  const sdk = manifest.artifacts['pow-captcha.js'];
  return injectScript(`${endpoint}${sdk.url}`, {
    integrity: sdk.integrity,
    crossOrigin: 'anonymous',
  });
}
```

无需 Ed25519 公钥配置（Tier 2 才涉及）。

### 下一步

- [ ] 发 Portcullis v1.1.3（可选，Tier 1 不破坏 v1.x 兼容）
- [ ] 通知主站团队（M3u8Preview_Go）对接 manifest
- [ ] 观察 1–2 个月线上 `/sdk/manifest.json` 请求率与 `integrity` 违例率
- [ ] 视主站需求再评估 Tier 2（Ed25519 签名）
