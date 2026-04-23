# Changelog

## [未发布] — SDK 加固 Tier 1

基于主站接入方提出的加固建议（`docs/CAPTCHA_SDK_HARDENING.md`），为 SDK 分发增加运行时可验证的 SRI 清单与版本化只读路径。**向后兼容**：旧 `/sdk/*file` 路径保留不变。

### 新增
- **`GET /sdk/manifest.json`** — 返回 `{version, builtAt, artifacts}`，每个 artifact 含 `url` / `sha384-<base64>` integrity / `size`。主站可据此做 `<script integrity=...>` 加载。
- **`GET /sdk/v{version}/*file`** — 版本化只读路径，`Cache-Control: public, max-age=31536000, immutable`，版本来自 `CARGO_PKG_VERSION`。
- **`Cross-Origin-Resource-Policy: cross-origin`** 头加到所有 SDK 资源与 manifest 响应，配合主站启用 COEP 时免手动放通。
- **SHA-384 integrity** 计算：rust-embed 嵌入字节编译期入 `OnceLock`，与原有 SHA-256 ETag 并存。
- **`BUILD_TIMESTAMP`** build.rs 环境变量，供 manifest `builtAt` 字段使用。

### 保留
- `GET /sdk/*file` 原路径照常工作，`Cache-Control: public, max-age=3600`，不带 SRI。升级期主站若命中旧 manifest 可 fallback 到此路径。

### 文档
- `docs/CAPTCHA_SDK_HARDENING.md`（威胁定义与方案对比）
- `docs/TIER1_IMPLEMENTATION.md`（实施进度与完成日志、端点清单、主站对接示意）

### 未做（Tier 2，视主站需求再评估）
- manifest 的 Ed25519 签名 + 管理面板公钥导出 UI

---

## [1.1.0] — 2026-04-22

**React 管理面板 + Docker Compose 多服务架构。**

### 新增
- **React 管理面板（admin-ui/）**
  - Vite + React 18 + TypeScript + Tailwind CSS + @tanstack/react-query
  - 5 个页面：登录 / 监控 / 站点管理 / 请求日志 / IP 风控
  - Token 认证，5 秒自动刷新，构建产物 250 KB / gzip 79 KB
- **管理 API（/admin/api/*）**
  - `GET /admin/api/stats` — 实时指标
  - `GET/POST /admin/api/sites` + `PUT/DELETE /admin/api/sites/:key` — 站点 CRUD
  - `GET /admin/api/logs` — 请求日志（最近 200 条）
  - `GET /admin/api/risk/ips` — IP 风控状态
  - `POST/DELETE /admin/api/risk/block` — 封禁 / 解封 IP
- **请求日志环形缓冲**：500 条容量，每次 `/verify` 写入 IP/site_key/nonce/状态/耗时
- **`[admin]` 配置段** + `CAPTCHA_ADMIN_TOKEN` 环境变量
- **Docker Compose 3 服务架构**：
  - `captcha-server` — Rust 验证服务
  - `admin-ui` — React SPA（Nginx 静态托管）
  - `nginx` — 网关（路由分发 `/admin/api → server`、`/admin → admin-ui`、`/ → server`）
- **`nginx/` 目录** — 网关 Nginx 配置 + Dockerfile

### 移除
- `crates/captcha-server/src/admin/dashboard.html` — 旧的 `include_str!` 嵌入式 HTML 面板（已替换为 React SPA）

---

## [1.0.0] — 2026-04-22

**生产就绪里程碑。`/api/v1/*` 接口格式冻结。**

### 新增
- criterion 性能基准（`cargo bench -p captcha-core`）
- CHANGELOG / UPGRADING / API_STABILITY 文档
- 所有 crate 版本号统一升至 1.0.0

### 包含的全部功能（v0.1.0 ~ v0.6.0 累积）

#### 核心算法（captcha-core）
- Argon2id + SHA-256 双阶段 PoW
- HMAC-SHA256 挑战签名（常数时间校验）
- Argon2 OnceLock 全局单例

#### 服务端（captcha-server）
- `/api/v1/challenge` — 发放挑战（无状态）
- `/api/v1/verify` — 校验解答 → captcha_token
- `/api/v1/verify/batch` — 批量校验（最多 20 条）
- `/api/v1/siteverify` — 业务后端核验 token（常数时间 secret_key 比较 + 单次使用）
- `/sdk/*` — 嵌入式 SDK + WASM 静态资源（ETag / 304 / gzip+br 压缩）
- `/metrics` — Prometheus 指标
- `/healthz` — 健康检查

#### 安全
- CORS 按站点白名单收窄
- IP 限流（governor 令牌桶，5 req/s burst 20）
- 安全响应头（X-Content-Type-Options / X-Frame-Options / Referrer-Policy）
- secret_key ≥ 16 字节校验
- 请求体 256 KiB 上限 + site_key 64 字节上限
- Origin header 白名单校验

#### 智能风控
- IP 动态难度（滑动窗口失败率 → 自动拉高 diff）
- IP 黑白名单（CIDR 支持）
- 配置热重载（30s 轮询 mtime）

#### 部署
- 单二进制（rust-embed 编译期嵌入 SDK + WASM）
- TOML 配置 + 环境变量 + CLI 参数三源加载
- clap CLI（gen-config / gen-secret / healthcheck 子命令）
- Dockerfile 四阶段构建 + docker-compose
- GitHub Actions CI + Release（4 平台二进制 + Docker ghcr.io）

#### SDK（浏览器端）
- 一行 `<script>` 自动挂载（data 属性驱动）
- chunked 主线程求解（无 Web Worker，无跨源限制）
- ARIA 无障碍
- 移动端响应式
- 网络重试 + 超时
- WASM 不支持检测
- IIFE 单文件 ~11 KB / gzip ~4.3 KB

#### 文档
- 协议规范（PROTOCOL.md）
- 接入指南（INTEGRATION.md）
- 安全加固（SECURITY.md）
- OpenAPI 3.1 规范（openapi.yaml）
- 7 种语言后端代码片段（snippets/）
- Grafana 面板模板（grafana-dashboard.json）
- 优化路线图（ROADMAP.md）

---

## [0.6.0] — 2026-04-22
- IP 动态难度 + 配置热重载 + IP 黑白名单
- 批量 verify 端点 + Grafana 面板模板
- AppState.config 改为 ArcSwap（lock-free 读）

## [0.5.0] — 2026-04-22
- Store trait 抽象 + Prometheus /metrics + 非阻塞日志
- MemoryStore 容量上限 + ETag/304 + OpenAPI + Argon2 OnceLock

## [0.4.0] — 2026-04-22
- ARIA 无障碍 + 移动端适配 + 网络重试+超时
- WASM 检测 + 多 endpoint cache + destroy 清理

## [0.3.0] — 2026-04-22
- GitHub Actions CI/Release + Dockerfile 修复 + healthcheck 子命令
- 死代码清理 + 响应压缩 + 缓存策略 + 移除热路径 panic

## [0.2.0] — 2026-04-22
- CORS 收窄 + IP 限流 + 常数时间比较 + token 单次使用
- 安全响应头 + secret_key 校验 + 请求体限制

## [0.1.0] — 2026-04-22
- 初始版本：captcha-core + captcha-server + captcha-wasm + SDK
