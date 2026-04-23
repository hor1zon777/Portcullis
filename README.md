# PoW CAPTCHA

基于工作量证明的验证码服务。用算力替代图像识别，用户点一下即完成验证。

**Argon2id 内存硬化 + SHA-256 快速迭代 | 单二进制 | 一行 `<script>` 接入 | 零第三方依赖**

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
docker compose up -d
# → http://localhost/admin/   管理面板
# → http://localhost/api/...  公共 API
```

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
  │     Argon2id(id, salt) → base_hash       │  一次性 ~5ms               │
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
- Argon2id 内存硬化（4 MiB/次），GPU 单卡并发成本提高 100 倍
- HMAC-SHA256 常数时间签名校验
- Token 单次使用 + 5 分钟过期
- CORS 按站点白名单 + Origin 校验
- IP 限流（令牌桶，5 req/s burst 20）
- `secret_key` 常数时间比较，防时序攻击
- 安全头（X-Content-Type-Options / X-Frame-Options / Referrer-Policy）

### 智能风控
- IP 动态难度：失败率高的 IP 自动提升 diff
- IP 黑白名单（CIDR 支持）
- 配置热重载：修改 `captcha.toml` 后 30s 内自动生效

### 可观测
- Prometheus `/metrics` 端点（challenge/verify/siteverify 计数器 + 延迟直方图）
- [Grafana 面板模板](docs/grafana-dashboard.json)（7 面板开箱即用）
- 结构化日志（非阻塞 writer）

### 管理面板
- **React 可视化面板**（`/admin/`）：监控仪表盘 / 站点 CRUD / 请求日志 / IP 风控
- Token 认证 + 5 秒自动刷新
- 站点新增/删除立即热重载，IP 封禁/解封实时生效

### 部署
- **单二进制**：SDK + WASM 编译期嵌入，部署仅需一个文件
- **TOML 配置**：`captcha.toml` + 环境变量 + CLI 参数三源加载
- **Docker Compose**：3 服务（captcha-server + admin-ui + nginx 网关）
- **CI/CD**：GitHub Actions 自动构建 Linux/macOS/Windows + Docker 镜像
- **CLI 工具**：`gen-config` / `gen-secret` / `healthcheck`

### SDK（浏览器端）
- 一行 `<script>` + data 属性，零 JS 代码
- 无 Web Worker（chunked 主线程），无跨源限制
- ARIA 无障碍 + 移动端响应式
- WASM 检测 + 网络重试 + 超时
- IIFE 单文件 **11 KB** / gzip **4.3 KB**

---

## 配置

```toml
# captcha.toml
[server]
bind = "0.0.0.0:8787"
secret = "运行 captcha-server gen-secret 生成"
challenge_ttl_secs = 120
token_ttl_secs = 300

[[sites]]
key = "pk_test"
secret_key = "sk_test_secret_at_least16"
diff = 18
origins = ["https://example.com"]

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
| 14 | ~0.2s | ~0.5s | 评论、点赞 |
| 16 | ~0.5s | ~1.2s | 普通登录 |
| **18** | **~1s** | **~2.5s** | **默认（注册、找回密码）** |
| 20 | ~3s | ~8s | 高敏感操作 |

基准数据（native release）：Argon2 base hash **5.4ms** / SHA-256 迭代 **89ns** / 单次 verify **5.2ms**

---

## API 端点

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/v1/challenge` | POST | 发放挑战 |
| `/api/v1/verify` | POST | 提交解答 → captcha_token |
| `/api/v1/verify/batch` | POST | 批量校验（最多 20 条） |
| `/api/v1/siteverify` | POST | 业务后端核验 token |
| `/admin/api/*` | GET/POST/PUT/DELETE | 管理面板 API（需 Bearer Token） |
| `/admin/` | GET | React 管理面板（Docker Compose 模式） |
| `/sdk/manifest.json` | GET | SDK 版本 + SRI integrity 清单（主站按清单加载） |
| `/sdk/v{version}/*` | GET | 版本化只读路径（`immutable` 长缓存，配合 SRI） |
| `/sdk/*` | GET | 嵌入式 SDK + WASM（ETag/304/gzip，向后兼容路径） |
| `/metrics` | GET | Prometheus 指标 |
| `/healthz` | GET | 健康检查 |

> `/api/v1/*` 格式自 v1.0.0 起冻结。详见 [API 稳定性承诺](docs/API_STABILITY.md)。
> SDK 分发策略详见 [SDK 加固实施](docs/TIER1_IMPLEMENTATION.md)。

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

```bash
# 终端 A：Rust 验证服务
export CAPTCHA_SECRET="dev-secret-must-be-at-least-32-bytes!!"
export CAPTCHA_ADMIN_TOKEN="dev-admin-token"
export CAPTCHA_SITES='{"pk_test":{"secret_key":"sk_test_secret_at_least16","diff":18,"origins":["http://localhost:5173","http://localhost:5174"]}}'
cargo run -p captcha-server

# 终端 B：CAPTCHA SDK 开发服务器
cd sdk && pnpm install && pnpm dev
# → http://localhost:5173

# 终端 C：管理面板开发服务器（HMR）
cd admin-ui && pnpm install && pnpm dev
# → http://localhost:5174/admin/
```

## 测试

```bash
cargo test                          # 45 个 Rust 测试
cargo bench -p captcha-core         # 性能基准（criterion）
cargo clippy --workspace            # 静态分析（0 warnings）
cd sdk && pnpm type-check           # SDK 类型检查
```

## 文档

| 文档 | 说明 |
|------|------|
| [AI 接入提示词](docs/AI_INTEGRATION_PROMPT.md) | 给 AI 编程助手的接入指南（推荐首读） |
| [协议规范](docs/PROTOCOL.md) | 双阶段算法、签名格式、端点详情 |
| [接入指南](docs/INTEGRATION.md) | 部署、配置、前后端接入三种方式 |
| [部署教程](docs/DEPLOY.md) | 从零部署完整步骤 |
| [安全加固](docs/SECURITY.md) | 12 项威胁对策 + 配置检查清单 |
| [后端代码片段](docs/snippets/README.md) | Node/Python/Go/PHP/Java/C#/Ruby |
| [OpenAPI 规范](docs/openapi.yaml) | 全部端点的机器可读定义 |
| [升级指南](docs/UPGRADING.md) | 算法参数变更的蓝绿发布步骤 |
| [API 稳定性](docs/API_STABILITY.md) | v1.x 兼容性承诺 |
| [变更日志](CHANGELOG.md) | 全部版本变更历史 |

## License

MIT
