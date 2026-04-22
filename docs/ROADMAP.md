# Roadmap

> 基于 v0.1.0（`4371bd7`）全量审计生成，按版本号组织。

---

## v0.2.0 — 安全加固（Critical）

| # | 项目 | 位置 | 说明 |
|---|------|------|------|
| 1 | **CORS 按站点收窄** | `lib.rs:16` | `SiteConfig.origins` 已配置但未生效，当前是 `allow_origin(Any)` |
| 2 | **IP 限流** | 全局缺失 | 加 `tower-governor`，按 IP 限 `/challenge` 和 `/verify`，需先实现 IP 提取（`X-Forwarded-For` + `ConnectInfo`） |
| 3 | **`secret_key` 常数时间比较** | `siteverify.rs:48` | 当前用 `==`，可被时序攻击逐字节爆破，改用 `subtle::ConstantTimeEq` |
| 4 | **`/siteverify` token 单次使用** | `siteverify.rs` | 同一 token 可反复核验直到过期，应在 MemoryStore 中加 `token_used` 集合 |
| 5 | **安全响应头** | `static_assets.rs` | 加 `X-Content-Type-Options: nosniff`、`X-Frame-Options: DENY`、CSP |
| 6 | **`secret_key` 最小长度校验** | `config.rs` | 配置加载时校验 ≥ 16 字节，拒绝弱密钥 |
| 7 | **请求体大小限制** | `routes/challenge.rs` | `site_key` 无长度上限，恶意发送 10MB 字符串 |

---

## v0.3.0 — 构建与质量

| # | 项目 | 位置 | 说明 |
|---|------|------|------|
| 1 | **GitHub Actions CI** | `.github/workflows/` | clippy + test + `pnpm build` + Docker 镜像推送 + 跨平台 Release |
| 2 | **Dockerfile 修复** | `Dockerfile` | 遗漏拷贝 `Cargo.lock` 和 `pnpm-lock.yaml`，导致不可复现构建 |
| 3 | **Docker 健康检查修复** | `docker-compose.yml:13` | 当前用 `--help`，改为 HTTP 探测 `/healthz`（加 `healthcheck` 子命令） |
| 4 | **清理死代码** | 多处 | `WorkerRequest/Response` 类型（types.ts）、`is_empty()`（memory.rs）、`from_env()` 改 `#[cfg(test)]` |
| 5 | **`sdk/package.json` 元数据补全** | `sdk/package.json` | `main` 指向不存在的 UMD 文件，缺 `exports`/`license`/`repository`/`keywords` |
| 6 | **响应压缩** | `lib.rs` | tower-http 加 `CompressionLayer`（gzip/br），`/sdk/*.js` 12-130 KB 受益明显 |
| 7 | **静态资源缓存策略修正** | `static_assets.rs:34` | `immutable` 与非哈希文件名矛盾，改为 `max-age=3600` 或加内容哈希 |
| 8 | **去掉请求路径上的 `.unwrap()`/`.expect()`** | `token.rs:23`、`static_assets.rs:40` | 改为返回 500 错误，避免 panic 打崩 worker |

---

## v0.4.0 — SDK 体验

| # | 项目 | 说明 |
|---|------|------|
| 1 | **无障碍（ARIA）** | 加 `aria-label`、`aria-live`、`aria-busy`；进度条加 `role="progressbar"` + `aria-valuenow` |
| 2 | **移动端适配** | `width: 300px` → `max-width: 100%` + `@media` 断点 |
| 3 | **网络重试 + 超时** | `api.ts` 加 `AbortController` 超时 10s + 1 次自动重试 |
| 4 | **WASM 降级方案** | 不支持 WASM 时显示「浏览器不兼容」提示，或 fallback 到纯 JS SHA-256（hash-wasm） |
| 5 | **`auto-mount.ts` 补全 `data-diff`** | 文档声明了但未读取，实现覆盖 `maxIters` 的逻辑 |
| 6 | **多 endpoint 场景** | `wasmCache` 按 `wasmBase` 分 key，避免多实例串 WASM |
| 7 | **`destroy()` 清理 setTimeout** | 存储 timeout ID 并在销毁时 clearTimeout |
| ~~8~~ | ~~npm 正式发布~~ | 已取消，SDK 仅通过服务端内嵌分发 |

---

## v0.5.0 — 分布式与可观测

| # | 项目 | 说明 |
|---|------|------|
| 1 | **Redis 存储后端** | 抽象 `Store` trait，实现 Redis 后端（`fred` / `redis` crate），支持多实例 |
| 2 | **Prometheus 指标** | `/metrics` 端点：challenge 发放速率、verify 成功/失败率、平均解题时间、活跃 challenge 数 |
| 3 | **结构化日志优化** | `tracing-subscriber` 加 `NonBlocking` writer，避免高负载下阻塞 tokio |
| 4 | **MemoryStore 容量上限** | 加 max-size 防 OOM（当前无界 DashMap） |
| 5 | **ETag / 304** | 静态资源加 `ETag` 响应头 + `If-None-Match` 处理 |
| 6 | **OpenAPI 规范** | 生成 `openapi.yaml`，覆盖 `/challenge`、`/verify`、`/siteverify`、`/healthz`、`/sdk/*` |
| 7 | **Argon2 单例** | `build_argon2()` 改用 `OnceLock<Argon2>` 避免每次请求重建 |

---

## v0.6.0 — 智能风控

| # | 项目 | 说明 |
|---|------|------|
| 1 | **IP 风控动态难度** | 按 IP 历史 verify 失败率 / 频率自动拉高 diff |
| 2 | **配置热重载** | 监听 `captcha.toml` 文件变更或 `SIGHUP` 信号，无需重启 |
| 3 | **IP 黑白名单** | 管理 API 或配置项，直接放行/拒绝指定 IP 段 |
| 4 | **Grafana 面板模板** | 基于 Prometheus 指标的预制 Dashboard JSON |
| 5 | **批量 verify 端点** | `POST /api/v1/verify/batch`，一次提交多个 nonce |
| 6 | **WebHook 通知** | 异常事件（verify 失败率飙升、限流触发）推送到配置的 URL |

---

## v1.0.0 — 生产就绪

| # | 项目 | 说明 |
|---|------|------|
| 1 | **浏览器 E2E 测试** | Playwright 覆盖 widget 全生命周期（点击 → 挖矿 → 提交 → token 填充） |
| 2 | **WASM 单元测试** | `wasm-bindgen-test` 覆盖 `create_solver`/`step`/`solve` |
| 3 | **性能基准** | `criterion` 跑 `verify_solution` 吞吐、Argon2 单次耗时 |
| 4 | **Fuzz 测试** | `cargo-fuzz` 覆盖 HMAC/serde 边界 |
| 5 | **API 稳定性承诺** | 冻结 `/api/v1/*` 请求/响应格式，后续变更走 `/api/v2/*` |
| 6 | **CHANGELOG** | 所有版本变更记录 |
| 7 | **UPGRADING 文档** | 算法参数变更的蓝绿/灰度发布指南 |
| ~~8~~ | ~~React / Vue 组件包~~ | 已取消，SDK 通过 `<script>` 标签 + `window.PowCaptcha` 全局 API 覆盖所有框架 |

---

## 总计

- **7 个版本**，**~45 项**优化
- 建议优先推 **v0.2.0**（安全）和 **v0.3.0**（CI/构建），完成后才适合对外开放使用
- v0.4.0-v0.6.0 可根据业务需求选择性推进
- v1.0.0 是 API 冻结与生产承诺的里程碑
