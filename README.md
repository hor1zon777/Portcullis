# PoW CAPTCHA

基于工作量证明的验证码服务。用算力替代图像识别，用户点一下即完成验证。

**Argon2id 内存硬化 + SHA-256 快速迭代 | 单二进制 | 一行 `<script>` 接入 | 零第三方依赖**

> **当前版本：v1.5.0** · 服务端密钥 HMAC 化 · admin 操作审计 · 双 key 轮换 · webhook 通知

---

## 为什么选 PoW CAPTCHA

| | 传统图像验证码 | PoW CAPTCHA |
|---|---|---|
| 用户体验 | 选图/拼图/滑块，耗时 5-15s | 点击后等 1-2s，全自动 |
| 隐私 | 发请求到 Google/hCaptcha | 自托管，零外部请求，零追踪 |
| GPU 攻击者成本 | 图像识别已被打码平台攻破 | 每次尝试需 4 MiB 内存，GPU 并发受限 |
| 部署 | 依赖第三方 API + 密钥 | 单二进制，内嵌 SDK，5 分钟部署 |
| 合规 | 向第三方传输用户数据 | 无 Cookie，无指纹，符合 GDPR/CCPA |

---

## 30 秒快速开始

### 1. 启动服务

```bash
# 方式 A：单二进制
./captcha-server gen-config > captcha.toml
./captcha-server gen-secret  # 将输出填入 captcha.toml 的 secret 字段
./captcha-server --config captcha.toml

# 方式 B：Docker Compose（3 服务：server + admin-ui + nginx）
mkdir -p data          # 创建数据目录
docker compose up -d
# → http://localhost/admin/   管理面板
# → http://localhost/api/...  公共 API
```

<details>
<summary>Docker 部署权限问题排查</summary>

#### 症状：captcha-server 启动失败，日志报 `unable to open database file: /data/captcha.db`

**原因**：容器内进程无权写入宿主机挂载的 `./data` 目录。

**解决方法**（任选一种）：

```bash
# 方法 1：确保 data 目录存在且可写
mkdir -p data
chmod 777 data
docker compose up -d

# 方法 2：如果使用了 nonroot 用户（UID 65532），需匹配权限
mkdir -p data
chown -R 65532:65532 data
chmod 700 data
docker compose up -d
```

#### 症状：admin-ui 或 nginx 报 `Permission denied` 或 `read-only file system`

**原因**：容器以非 root 用户运行，无法创建 nginx 缓存目录。

**解决方法**：确保 `docker-compose.yml` 中 admin-ui 和 nginx 服务没有设置 `user` 和 `read_only` 限制，或使用默认配置：

```yaml
# docker-compose.yml（无权限限制的最简配置）
services:
  captcha-server:
    image: ghcr.io/hor1zon777/portcullis/server:latest
    env_file: .env
    environment:
      - CAPTCHA_DB_PATH=/data/captcha.db
    volumes:
      - ./data:/data
    expose:
      - "8787"
    restart: unless-stopped

  admin-ui:
    image: ghcr.io/hor1zon777/portcullis/admin-ui:latest
    expose:
      - "80"
    restart: unless-stopped

  nginx:
    image: ghcr.io/hor1zon777/portcullis/nginx:latest
    ports:
      - "${NGINX_PORT:-80}:80"
    depends_on:
      - captcha-server
      - admin-ui
    restart: unless-stopped
```

</details>

### 2. 前端接入（零 JS 代码）

```html
<script src="https://your-captcha-server.com/sdk/pow-captcha.js"
        data-site-key="pk_test"></script>

<form action="/login" method="POST">
  <input name="username" />
  <input name="password" type="password" />
  <div data-pow-captcha data-target="captcha_token"></div>
  <input type="hidden" name="captcha_token" id="captcha_token" />
  <button type="submit">登录</button>
</form>
```

SDK 自动渲染 widget → 用户点击 → 后台挖矿 → 通过后自动填充 hidden input。

### 3. 后端校验（任选一种语言）

```bash
curl -X POST https://your-captcha-server.com/api/v1/siteverify \
  -H 'content-type: application/json' \
  -d '{"token":"<captcha_token>","secret_key":"sk_your_secret"}'
```

```json
{"success": true, "challenge_id": "...", "site_key": "pk_test"}
```

> 7 种语言的完整代码片段：[Node](docs/snippets/nodejs.md) · [Python](docs/snippets/python.md) · [Go](docs/snippets/go.md) · [PHP](docs/snippets/php.md) · [Java](docs/snippets/java.md) · [C#](docs/snippets/csharp.md) · [Ruby](docs/snippets/ruby.md)

---

## 工作原理

```
浏览器                                     验证服务                     业务后端
  │                                          │                            │
  │  ① POST /challenge {site_key}            │                            │
  ├─────────────────────────────────────────►│                            │
  │  ◄── {challenge, sig}                    │                            │
  │                                          │                            │
  │  ② 本地挖矿（Web Worker-free）           │                            │
  │     Argon2id(id, salt, m/t/p) →          │  一次性 ~20ms (默认 19 MiB) │
  │         base_hash                        │  m/t/p 由 challenge 下发    │
  │     SHA-256(base ‖ nonce) × N            │  迭代 ~1-2s                │
  │                                          │                            │
  │  ③ POST /verify {challenge, sig, nonce}  │                            │
  ├─────────────────────────────────────────►│                            │
  │  ◄── {captcha_token}                     │  验证仅需 1 次 Argon2+SHA  │
  │                                          │                            │
  │  ④ 表单提交携带 captcha_token ──────────────────────────────────────►│
  │                                          │  ⑤ POST /siteverify        │
  │                                          │◄────────────────────────────┤
  │                                          │──── {success: true} ──────►│
```

## 特性一览

### 安全
- Argon2id 内存硬化，**参数每 challenge 下发并经 HMAC 签名保护**（v1.3+），默认 19 MiB/2 轮
- HMAC-SHA256 常数时间签名校验，支持 `CAPTCHA_SECRET` 双 key 无缝轮换（v1.5+）
- `secret_key` 服务端 HMAC 化存储，DB 泄漏不再等于密钥泄漏（v1.5+）
- Token 单次使用 + 5 分钟过期；opt-in 绑定 client IP / User-Agent（v1.4+）
- CORS 按站点白名单 + Origin 校验
- IP 限流（令牌桶，5 req/s burst 20）；admin 登录 30 次失败触发 15 分钟 ban（v1.5+）
- 安全头（X-Content-Type-Options / X-Frame-Options / Referrer-Policy）

### SDK 完整性（v1.2.x）
- `/sdk/manifest.json`：版本 + SRI 清单，主站可 `<script integrity=...>` 固化加载
- 版本化只读路径 `/sdk/v{version}/*`，`Cache-Control: immutable` 长缓存
- opt-in Ed25519 `X-Portcullis-Signature`：管理员在面板一键生成/撤销密钥
- WASM 构建期 `--remap-path-prefix` + `--strip-producers`，二进制不泄漏构建机 PII

### 智能风控
- IP 动态难度：失败率高的 IP 自动提升 diff
- IP 黑白名单（CIDR 支持）
- 配置热重载：修改 `captcha.toml` 后 30s 内自动生效

### 可观测
- Prometheus `/metrics` 端点（challenge/verify/siteverify 计数器 + 延迟直方图）
- [Grafana 面板模板](docs/grafana-dashboard.json)（7 面板开箱即用）
- 结构化日志（非阻塞 writer）
- **admin 操作审计**（v1.5+）：站点 CRUD / IP 封解 / 密钥生成撤销 / 登录失败全量可追溯
- **admin webhook**（v1.5+）：关键操作可选推送 Slack Incoming Webhook 兼容端点

### 管理面板
- **React 可视化面板**（`/admin/`）：监控仪表盘 · 站点 · 日志 · 风控 · 安全 · **审计**（v1.5+）
- Token 认证 + 10 秒自动刷新，深色模式
- 站点新增/删除立即热重载，IP 封禁/解封实时生效
- 每站点可视化配置 Argon2 参数（m/t_cost）、Token 身份绑定开关
- 「安全」页一键生成/撤销 Ed25519 manifest 签名密钥
- 「审计」页按 action 过滤 + 分页，500 条/页

### 部署
- **单二进制**：SDK + WASM 编译期嵌入，部署仅需一个文件
- **TOML 配置**：`captcha.toml` + 环境变量 + CLI 参数三源加载
- **Docker Compose**：3 服务（captcha-server + admin-ui + nginx 网关）
- **CI/CD**：GitHub Actions 自动构建 Linux/macOS/Windows + Docker 镜像
- **CLI 工具**：`gen-config` / `gen-secret` / `gen-manifest-key` / `healthcheck`

### SDK（浏览器端）
- 一行 `<script>` + data 属性，零 JS 代码
- 无 Web Worker（chunked 主线程），无跨源限制
- ARIA 无障碍 + 移动端响应式
- WASM 检测 + 网络重试 + 超时
- IIFE 单文件 **13 KB** / gzip **5.3 KB**

---

## 配置

```toml
# captcha.toml
[server]
bind = "0.0.0.0:8787"
secret = "运行 captcha-server gen-secret 生成"
# v1.5+ 密钥轮换：临时双 key 平滑切换
# secret_previous = "上一代 secret"
challenge_ttl_secs = 120
token_ttl_secs = 300
# v1.2+ manifest 签名（可选，面板「安全」页一键管理更方便）
# manifest_signing_key = "运行 captcha-server gen-manifest-key"
# v1.5+ admin 操作 webhook（Slack 兼容）
# admin_webhook_url = "https://hooks.slack.com/services/..."

[[sites]]
key = "pk_test"
secret_key = "sk_test_secret_at_least16"
diff = 18
origins = ["https://example.com"]
# v1.3+ Argon2 参数逐站点覆盖（管理面板可视化编辑）
# argon2_m_cost = 19456  # KiB，范围 [8, 65536]
# argon2_t_cost = 2
# argon2_p_cost = 1
# v1.4+ opt-in token 身份绑定（需反代正确透传 XFF / X-Real-IP）
# bind_token_to_ip = false
# bind_token_to_ua = false

[risk]
dynamic_diff_enabled = true
dynamic_diff_max_increase = 4
fail_rate_threshold = 0.7
blocked_ips = []
allowed_ips = ["127.0.0.1"]

[admin]
enabled = true
token = "运行 captcha-server gen-secret 生成"
```

> 完整配置模板：[captcha.toml.example](captcha.toml.example)

## 难度参考

| diff | 桌面端 | 移动端 | 适用场景 |
|------|--------|--------|---------|
| 14 | ~0.5s | ~1.2s | 评论、点赞 |
| 16 | ~1.2s | ~3s | 普通登录 |
| **18** | **~2s** | **~5s** | **默认（注册、找回密码）** |
| 20 | ~6s | ~15s | 高敏感操作 |

基准数据（v1.5.0，默认 Argon2id 19456/2/1，native release）：单次 Argon2 ~20ms / SHA-256 迭代 ~90ns / 单次 verify ~20ms

> Argon2 参数可在管理面板按站点调整（v1.3+）。耗时参考：`4096/1/1`≈5ms、`19456/2/1`≈20ms（默认）、`65536/4/1`≈200ms。

---

## API 端点

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/v1/challenge` | POST | 发放挑战（响应含 Argon2 参数 m/t/p_cost，v1.3+） |
| `/api/v1/verify` | POST | 提交解答 → captcha_token（按 site 开关绑定 IP/UA，v1.4+） |
| `/api/v1/verify/batch` | POST | 批量校验（最多 20 条） |
| `/api/v1/siteverify` | POST | 业务后端核验 token（可传 client_ip / user_agent，v1.4+） |
| `/admin/api/sites` | GET/POST | 站点列表 / 创建（创建响应含一次性明文 secret_key，v1.5+） |
| `/admin/api/sites/:key` | PUT/DELETE | 站点更新 / 删除 |
| `/admin/api/logs` | GET | 请求日志（最近 200 条） |
| `/admin/api/risk/ips` | GET | IP 风控状态 |
| `/admin/api/risk/block` | POST/DELETE | IP 封禁 / 解封 |
| `/admin/api/manifest-pubkey` | GET/DELETE | 查询 / 撤销 Ed25519 manifest 公钥 |
| `/admin/api/manifest-pubkey/generate` | POST | 一键生成 manifest 签名密钥对 |
| `/admin/api/audit` | GET | 管理员操作审计（v1.5+，支持 `?action=` 过滤） |
| `/admin/` | GET | React 管理面板（Docker Compose 模式） |
| `/sdk/manifest.json` | GET | SDK 版本 + SRI integrity 清单；opt-in 带 Ed25519 签名 |
| `/sdk/v{version}/*` | GET | 版本化只读路径（`immutable` 长缓存，配合 SRI） |
| `/sdk/*` | GET | 嵌入式 SDK + WASM（ETag/304/gzip，向后兼容路径） |
| `/metrics` | GET | Prometheus 指标 |
| `/healthz` | GET | 健康检查 |

> `/api/v1/*` 格式自 v1.0.0 起冻结。新增字段向后兼容，详见 [API 稳定性承诺](docs/API_STABILITY.md)。
> SDK 分发策略详见 [Tier 1 实施](docs/TIER1_IMPLEMENTATION.md)、[Tier 2 实施](docs/TIER2_IMPLEMENTATION.md)。

## 项目结构

```
captcha/
├── crates/
│   ├── captcha-core/       Argon2id + SHA-256 + HMAC（共享算法库）
│   ├── captcha-server/     axum HTTP 服务 + 嵌入式静态资源 + Admin API
│   └── captcha-wasm/       浏览器端 WASM 求解器
├── sdk/                    浏览器 SDK（TypeScript IIFE 单文件）
├── admin-ui/               React 管理面板（Vite + Tailwind + react-query）
├── nginx/                  Docker 网关 Nginx 配置
├── docs/                   协议/接入/安全/部署/升级/API 稳定性/OpenAPI/Grafana
├── Dockerfile              captcha-server 构建
├── docker-compose.yml      3 服务编排（server + admin-ui + nginx）
├── captcha.toml.example    配置模板
├── CHANGELOG.md            版本变更历史
└── scripts/                构建与开发脚本
```

## 从源码构建

```bash
# 前置要求：Rust 1.70+、Node 18+、pnpm、wasm-pack
bash scripts/build-all.sh
# 输出：target/release/captcha-server（单二进制，含嵌入的 SDK + WASM）
```

Windows 用户：
```powershell
.\scripts\build-all.ps1
```

## 开发模式

**一条命令、一个端口**：根目录 `package.json` 用 `concurrently` 同时拉起 Rust 服务和 admin-ui Vite，浏览器只需访问 `http://localhost:5173`。

```bash
# 首次：安装根 + admin-ui 依赖（仅一次）
pnpm dev:setup

# 启动（任意 OS / 任意终端）
pnpm dev
```

启动后：

| 入口 | URL |
|------|-----|
| 管理后台 | <http://localhost:5173/admin/>（admin token 默认 `dev-admin-token`）|
| 公共 API | <http://localhost:5173/api/v1/...> |
| SDK 资源 | <http://localhost:5173/sdk/manifest.json> |
| 健康检查 | <http://localhost:5173/healthz> |

实际监听：

- Rust 服务：`127.0.0.1:8787`（仅 loopback，配置来自 `captcha.dev.toml`）
- Vite：`127.0.0.1:5173`，把 `/api`、`/admin/api`、`/sdk`、`/healthz`、`/metrics` 反代到 8787

`Ctrl+C` 同时退出两个进程。如需 SDK 源码 HMR，另起 `pnpm -C sdk dev`。

<details>
<summary>手动分进程启动</summary>

```bash
# 终端 A：Rust
cargo run -p captcha-server -- --config captcha.dev.toml

# 终端 B：admin-ui
pnpm -C admin-ui dev
```

</details>

## 测试

```bash
cargo test                          # 119 个 Rust 测试（27 core + 39 server unit + 43 integration + 10 site_secret/crypto 新增）
cargo bench -p captcha-core         # 性能基准（criterion）
cargo clippy --workspace            # 静态分析（0 warnings）
cd sdk && pnpm type-check           # SDK 类型检查
cd admin-ui && pnpm build           # 管理面板产线构建
```

## 文档

| 文档 | 说明 |
|------|------|
| [AI 接入提示词](docs/AI_INTEGRATION_PROMPT.md) | 给 AI 编程助手的接入指南（推荐首读） |
| [协议规范](docs/PROTOCOL.md) | 双阶段算法、签名格式、端点详情 |
| [接入指南](docs/INTEGRATION.md) | 部署、配置、前后端接入三种方式 |
| [部署教程](docs/DEPLOY.md) | 从零部署完整步骤（含反代 XFF 透传、webhook、审计） |
| [安全加固](docs/SECURITY.md) | 威胁对策与配置检查清单 |
| [后端代码片段](docs/snippets/README.md) | Node/Python/Go/PHP/Java/C#/Ruby |
| [OpenAPI 规范](docs/openapi.yaml) | 全部端点的机器可读定义 |
| [升级指南](docs/UPGRADING.md) | 逐版本升级步骤（含 secret_key 迁移、双 key 轮换） |
| [API 稳定性](docs/API_STABILITY.md) | v1.x 兼容性承诺 |
| [路线图](docs/ROADMAP.md) | 各版本范围与下一步计划 |
| [变更日志](CHANGELOG.md) | 全部版本变更历史 |

## License

MIT
