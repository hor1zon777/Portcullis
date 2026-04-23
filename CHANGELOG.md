# Changelog

## [1.2.5] — 2026-04-24

### 修复（安全硬化）
- **WASM 二进制泄露本机 PII 路径**：发布的 `captcha_wasm_bg.wasm` 里嵌入了构建机的完整路径（`C:\Users\<name>\.cargo\...`、`.rustup\...`），每次部署都会暴露构建者用户名。攻击者 `strings` 一下就能拿到。
- 同时含 `producers` custom section（标识 rustc + wasm-bindgen 精确版本），方便攻击者针对特定版本构造 exploit。
- 基于一份外部安全评估（含 WASM 字节扫描 + 实证逆向）的 P0-3 条修复。

### 实施
- 新增 `.cargo/config.toml`：Linux CI / Docker / macOS / Windows 常见构建机的 `$CARGO_HOME` / `$RUSTUP_HOME` 都做 `--remap-path-prefix` 脱敏 → `/cargo` / `/rustup`。
- workspace `Cargo.toml` `[profile.release]` 加 `strip = "symbols"`：移除 ELF/PE 符号表。
- `crates/captcha-wasm/Cargo.toml` 加 `[package.metadata.wasm-pack.profile.release]`：wasm-pack 构建时 wasm-opt 带 `--strip-debug --strip-producers --vacuum`，清 DWARF + producers section。

### 验证
- rebuild 后 `strings sdk/pkg/captcha_wasm_bg.wasm | grep -i "Captain"` → 空
- `strings | grep -E "rustc|processed-by|wasm-bindgen v"` → 空（producers section 已删）

### 已知残留（本版本不处理）
- crate 版本号路径（如 `argon2-0.5.3/src/params.rs`）仍可见。要彻底清除需 `panic = "abort"`，但会让 server handler panic 时直接杀进程，代价过高。v1.3.0 通过 PoW 参数下发化协议（challenge 自带 m/t/p 且 HMAC 签名覆盖）让攻击者即使知道 crate 版本也无法预计算，从而 neutralize 这条残留的攻击价值。

### 无功能变更
- 后端协议零改动；所有 crate 版本号同步到 1.2.5。

---

## [1.2.4] — 2026-04-23

### 修复
- **移动端管理面板 iOS Safari 自动放大页面**：输入框聚焦时 iOS Safari 会自动 zoom，原因是 `.input` 基础字号是 14px (`text-sm`)，低于 iOS 的 16px 阈值。改为 `text-base md:text-sm`（移动端 16px、md+ 保持 14px 紧凑外观），登录、站点、日志、风控、安全页共用的所有输入框/下拉选择一次修掉。
- **登录页在超小屏溢出**：`w-96` (384px) 在 360px 宽屏幕会超出，改为 `w-full max-w-sm p-4`。

### 无功能变更
- 后端零改动；crate 版本号跟进到 1.2.4 仅为保持前后端统一。

---

## [1.2.3] — 2026-04-23

### 新增
- **一键生成 / 重新生成 / 停用 manifest 签名密钥**，完全在管理面板「安全」页完成，无需命令行与改环境变量。
  - `POST /admin/api/manifest-pubkey/generate`：服务端随机 32 字节 seed → 写 SQLite `server_secrets` 表 → 更新 ArcSwap 配置 → 返回公钥。热生效，`/sdk/manifest.json` 立刻开始签名。
  - `DELETE /admin/api/manifest-pubkey`：删除 DB 中的 seed + 置空配置。幂等（`removed: bool` 标记是否真删了）。
  - 覆盖语义：若已有密钥，生成即替换；响应 `first_time: false`。
- SQLite 新增 `server_secrets` 表存放 32 字节长寿秘密。
- `AppState::reload_config` 合并时以 DB 中的 signing key 为准，防止配置热重载覆盖管理面板生成/撤销的结果。

### 前端
- 「安全」页「未配置」状态显示「一键生成密钥对」大按钮 + 操作说明卡片。
- 「已启用」状态新增「重新生成」「停用」按钮，均有 `ConfirmDialog` 二次确认（标红 destructive 样式）。
- 所有操作成功后 `react-query invalidate`，页面自动刷新到最新状态。

### 兼容
- 原 env `CAPTCHA_MANIFEST_SIGNING_KEY` / toml `[server].manifest_signing_key` 依然有效，**首次启动**时会 seed 到 DB，之后 DB 为 source of truth。
- 已有 v1.2.0 ~ v1.2.2 的部署升级后，若之前用 env/toml 配过密钥，首次启动自动导入 DB；之后管理员可直接在面板上重新生成或停用。

---

## [1.2.2] — 2026-04-23

### 新增
- **管理面板「安全」页** (`/admin/security/`)：显示 Manifest 签名公钥（带复制按钮），以及"未启用"状态下的配置引导。补齐 v1.2.0 Tier 2 遗留的 UI（当时只做了 `GET /admin/api/manifest-pubkey` API）。
- 侧边栏导航新增「安全」入口（KeyRound 图标）。

### 无功能变更
- 后端零改动；crate 版本号跟进到 1.2.2 仅为保持与前端统一。

---

## [1.2.1] — 2026-04-23

### 修复
- `cargo fmt` 格式化，解决 v1.2.0 在 CI `cargo fmt --all -- --check` 阶段的失败（本地遗漏运行 fmt 导致）
- 无功能变更；仅代码风格

---

## [1.2.0] — 2026-04-23 — SDK 加固 Tier 1 + Tier 2

基于主站接入方提出的加固建议（`docs/CAPTCHA_SDK_HARDENING.md`），分两档实施。**全部向后兼容**：旧 `/sdk/*file` 路径保留不变；Tier 2 签名 opt-in，不配置密钥时行为与 Tier 1 一致。

### Tier 1 — SRI 清单与版本化路径

- **`GET /sdk/manifest.json`** — 返回 `{version, builtAt, artifacts}`，每个 artifact 含 `url` / `sha384-<base64>` integrity / `size`。主站可据此做 `<script integrity=...>` 加载。
- **`GET /sdk/v{version}/*file`** — 版本化只读路径，`Cache-Control: public, max-age=31536000, immutable`，版本来自 `CARGO_PKG_VERSION`。
- **`Cross-Origin-Resource-Policy: cross-origin`** 头加到所有 SDK 资源与 manifest 响应，配合主站启用 COEP 时免手动放通。
- **SHA-384 integrity** 计算：rust-embed 嵌入字节编译期入 `OnceLock`，与原有 SHA-256 ETag 并存。
- **`BUILD_TIMESTAMP`** build.rs 环境变量，供 manifest `builtAt` 字段使用。

### Tier 2 — Ed25519 签名 manifest

- **`X-Portcullis-Signature`** 响应头：base64 编码的 Ed25519 签名（对 manifest response body 原始字节签名），配置了私钥时发出，未配置时缺失（向后兼容）。
- **`CAPTCHA_MANIFEST_SIGNING_KEY`** env / `[server].manifest_signing_key` toml：Ed25519 32 字节 seed 的 base64。
- **`captcha-server gen-manifest-key`** CLI 子命令：生成密钥对并分行输出 seed / 公钥，供带外配置。
- **`GET /admin/api/manifest-pubkey`**（需 admin token）：返回 `{enabled, pubkey, algorithm}`，供管理员复制公钥到主站配置。
- 依赖新增：`ed25519-dalek = "2"`（`default-features = false` + `std` + `fast`）。

### 保留
- `GET /sdk/*file` 原路径照常工作，`Cache-Control: public, max-age=3600`，不带 SRI / 签名。升级期主站若命中旧 manifest 可 fallback 到此路径。

### 文档
- `docs/CAPTCHA_SDK_HARDENING.md`（威胁定义、方案对比、最新实施状态表）
- `docs/TIER1_IMPLEMENTATION.md`（Tier 1 进度与完成日志）
- `docs/TIER2_IMPLEMENTATION.md`（Tier 2 进度与完成日志、主站验签示例）
- `docs/INTEGRATION.md` — 新增"方式 D：带 SRI 的动态加载"与"方式 D+：验签升级"

### 未做（观察需要后再评估）
- 双 signing key 轮换（当前轮换靠两步部署：主站先认新公钥 → Portcullis 切换私钥）

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
