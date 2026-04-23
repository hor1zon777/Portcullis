# Roadmap

> **当前版本：v1.3.0** | 最后更新：2026-04-24

---

## 状态速览

| 阶段 | 版本 | 主题 | 状态 |
|---|---|---|---|
| 0 | v0.1.0 → v1.0.0 | 生产就绪（算法、SDK、CI/CD、文档） | ✅ 已完成 |
| 1 | v1.1.x | React 管理面板 + Docker Compose 三服务 | ✅ 已完成 |
| 2 | v1.2.0 → v1.2.5 | SDK 加固 Tier 1/2 + 管理 UI + PII 脱敏 | ✅ 已完成 |
| 3 | v1.3.0 | PoW 参数下发化 | ✅ 已完成 |
| **4** | **v1.4.0** | **CaptchaToken 客户端身份绑定（opt-in）** | 🔜 下一个 |
| 5 | v1.5.0 | 服务端密钥与审计硬化 | 规划中 |
| 6 | v1.6.0 | 供应链与分发签名 | 规划中 |
| 7 | v2.0.0 | 企业级（多实例 / RBAC / KMS） | 远期 |

历史版本详细变更见 [CHANGELOG.md](../CHANGELOG.md)。

---

## v1.3.0 — PoW 参数下发化（✅ 已完成）

> **目标**：让 Argon2id 参数从"编译期硬编码"变为"每 challenge 携带 + HMAC 签名"，彻底中和 WASM 里残留的 crate 版本号泄漏（v1.2.5 未能清除）的攻击价值。

### 背景

v1.2.5 把构建机 PII 路径清掉了，但 `argon2-0.5.3/src/params.rs` 这类 cargo registry 路径仍在 WASM 的 `.rodata` 里。单靠 `panic = "abort"` 能清但会让 server handler panic 直接杀进程，代价过高。

实证攻击成本：攻击者从 WASM `strings` 拿到 `argon2-0.5.3` → 确认算法 → 枚举 15 组常见 `(m, t, p)` 参数 → 命中 `(4096, 1, 1)` 仅需 5 分钟。解决路径不在"清理 crate 字符串"，而在**把参数从源码移到 challenge 里**，让每站点参数不同 + 随时可调 + 逆向无意义。

### 范围

| # | 项目 | 位置 | 说明 |
|---|---|---|---|
| 1 | **Challenge 扩展 m/t/p 字段** | `captcha-core/src/challenge.rs` | 加 `m_cost: u32`、`t_cost: u32`、`p_cost: u32`；`to_sign_bytes()` 纳入签名保护 |
| 2 | **向后兼容反序列化** | 同上 | 旧客户端产生的 JSON 无新字段时，`#[serde(default)]` 回填 `4096/1/1`，老部署升级期间不破坏流量 |
| 3 | **服务端按 challenge 参数重算** | `captcha-core/src/pow.rs` | 去掉全局 `OnceLock<Argon2>`；`compute_base_hash` 接收 `Params`，每 challenge 构建 Argon2 实例（冷启仍可 cache 按参数签名分组的实例） |
| 4 | **默认参数提档** | 同上 | `4096/1/1` → `19456/2/1`（OWASP 2024 推荐 Argon2id 第二档）。单次桌面 Argon2 从 5ms → ~20ms，可接受 |
| 5 | **SiteConfig 可配参数** | `config.rs` / `admin/handlers.rs` | 每站点 `argon2_m_cost/t_cost/p_cost` 覆盖默认；管理面板「站点」页加参数编辑列 |
| 6 | **WASM solver 按 challenge 重建 Argon2** | `captcha-wasm/src/*` | `create_solver` 已有 `payloadJson`，从中读 `m/t/p` 构建 Argon2。内存占用随 `m_cost` 变化，WASM `no_std` 版 Argon2id 已经支持自定义 params |
| 7 | **管理面板参数文档** | `admin-ui/src/pages/Sites.tsx` | 新增字段悬浮说明（耗时参考表） |
| 8 | **参数范围校验** | `config.rs` / handlers | `m_cost` ∈ [8, 65536]、`t_cost` ∈ [1, 10]、`p_cost` == 1（serial 场景 OK）；超范围拒绝 |
| 9 | **集成测试** | `tests/integration.rs` | 覆盖：新 Challenge 含参数 / 签名覆盖参数 / 篡改参数被拒 / 不同 site 不同参数各自生效 / 旧格式 JSON fallback 默认值 |
| 10 | **升级文档** | `docs/UPGRADING.md` | 蓝绿或灰度步骤；强调必须服务端和 SDK 同步升级（SDK 自动 via manifest.json → 只要先发服务端再 pnpm build） |

### 工作量估计：1 天（含迁移测试） — 实际约 1 天完成

### 交付清单（全部落地）
- ✅ Challenge 扩展 m/t/p 字段 + 签名覆盖
- ✅ 向后兼容反序列化（旧 JSON 回填 `4096/1/1`）
- ✅ 服务端按 challenge 参数重算（移除 `OnceLock<Argon2>`）
- ✅ 默认参数提档 `19456/2/1`
- ✅ SiteConfig 可配参数 + 管理面板参数编辑列
- ✅ WASM solver 按 challenge 重建 Argon2
- ✅ 管理面板参数文档（Tooltip + 耗时参考表）
- ✅ 参数范围校验（服务端 + 前端双重）
- ✅ 集成测试（6 个新用例覆盖所有场景）
- ✅ 升级文档 `docs/UPGRADING.md` 重写

### 破坏性说明
- **不破坏 `/api/v1/*` 外部 API**（Challenge 是 Portcullis 内部结构，SDK + 服务端共用）
- 从 Portcullis 1.2.x 升级到 1.3.0：如果管理员用的是 **Docker Compose 一键部署**（SDK 随 server 镜像），原子升级没问题
- 如果主站自己打包过 SDK（少见），需要重新 `pnpm build` 同步

---

## v1.4.0 — CaptchaToken 客户端身份绑定（opt-in）

> **目标**：阻断"一台机器批量解 PoW → 分发到多 IP 使用"的跨机器复用攻击。

### 范围

| # | 项目 | 说明 |
|---|---|---|
| 1 | **Token payload 加身份字段** | `token.rs`：可选 `ip_hash`（sha256 前 16 字节）、`ua_hash`（sha256 前 8 字节） |
| 2 | **SiteConfig 开关** | `bind_token_to_ip: bool`、`bind_token_to_ua: bool`，默认 false |
| 3 | **siteverify 校验来源** | 业务后端调用 siteverify 时新增可选字段 `client_ip` / `user_agent`；服务端和 token 里的 hash 比对 |
| 4 | **反代透传文档** | `docs/DEPLOY.md`：启用 `bind_token_to_ip` 时必须正确转发 `X-Real-IP` / `X-Forwarded-For`，否则流量全挂 |
| 5 | **管理面板开关 UI** | `admin-ui/src/pages/Sites.tsx` 加两个 checkbox |

### 工作量：2-3 天

### 设计取舍
Portcullis 的卖点之一是「自托管不收集用户数据」（README 对比 reCAPTCHA），引入 IP/UA hash 绑定语义上扩展了数据使用面。所以：
- **默认关闭**
- 文档清楚说明开启后的隐私权衡
- hash 只进 token（不写 DB、不写日志）

---

## v1.5.0 — 服务端密钥与审计硬化

### 范围

| # | 项目 | 破坏性 | 说明 |
|---|---|---|---|
| 1 | **DB `secret_key` 改 hash 存储** | ⚠️ schema migration | 当前 `sites.secret_key` 明文存 SQLite；改存 `HMAC-SHA256(CAPTCHA_SECRET, secret_key)`，siteverify 时 HMAC 请求值再常数时间比对。备份/DB 泄漏不再等于密钥泄漏 |
| 2 | **admin 操作审计表** | 新增表 | `admin_audit(id, ts, token_prefix, action, target, ip, meta_json)`；管理面板新增「审计」页。记录：站点 CRUD / IP 封解 / manifest key 生成撤销 / 登录成功失败 |
| 3 | **admin 登录限流 + IP ban** | 无破坏 | `/admin/api/*` 独立限流层（5 次/分钟，连续 30 次失败 15 分钟 ban）；写入 audit |
| 4 | **`CAPTCHA_SECRET` 双 key 轮换** | 无破坏 | 支持 `CAPTCHA_SECRET_PREVIOUS` env；verify 用 current 签发、同时接受 previous 签的未过期 token。平滑切换密钥 |
| 5 | **Admin 操作 webhook** | 可选 | 关键操作（manifest key 生成/撤销、CAPTCHA_SECRET 轮换）可选推送到配置的 URL（Slack incoming webhook 格式） |

### 工作量：3-5 天

### 迁移说明
`secret_key` hash 化是向后不兼容的 schema 变更。启动时检测到明文行自动一次性 hash，**这要求管理员在 v1.4.x 之前已经保留了 secret_key 的原文备份**（因为 hash 化后 DB 里拿不回原文）。升级流程要在 `UPGRADING.md` 强调这一点。

---

## v1.6.0 — 供应链与分发签名

### 范围

| # | 项目 | 说明 |
|---|---|---|
| 1 | **CI `cargo-audit` + `cargo-deny`** | `.github/workflows/security.yml` 每周 schedule 跑；advisory + license + bans 三层检查 |
| 2 | **Docker 镜像 non-root** | Dockerfile stage 4 加 `USER nonroot:nonroot`（distroless/cc 原生支持） |
| 3 | **Docker 镜像 cosign 签名** | `release.yml` 用 `cosign sign ghcr.io/...@sha256:...`；README 给出 verify 命令 |
| 4 | **二进制 cosign blob 签名** | 每个平台 tar.gz/zip 生成 `.sig` 附在 release，用户 `cosign verify-blob` 验签 |
| 5 | **`cargo-auditable` SBOM** | 二进制内嵌 SBOM；`docker scout` / `trivy` 直接扫描 |
| 6 | **Dependabot** | `.github/dependabot.yml` 开启 cargo + npm + docker + actions 每周更新 PR |
| 7 | **SECURITY.md 补充** | CVE 报告流程、PGP key、披露期限 |

### 工作量：2-3 天（多数是 CI 配置）

---

## v2.0.0 — 企业级（远期）

### 主题
多实例、多租户、合规要求

| # | 项目 | 工作量 | 说明 |
|---|---|---|---|
| 1 | **Redis store backend** | 1 周 | `Store` trait 抽出 redis 实现，支持多 captcha-server 实例共享 challenge / token / nonce，消灭进程级状态 |
| 2 | **RBAC + JWT admin 登录** | 1 周 | `read-only` / `operator` / `admin` 三档；admin token → 用户名密码 + JWT + refresh；可选 TOTP 2FA |
| 3 | **KMS/HSM 抽象** | 1 周 | `SigningBackend` trait 让 `CAPTCHA_SECRET` 和 `manifest_signing_key` 可以放 AWS KMS / GCP KMS / HashiCorp Vault；默认实现还是本地 bytes |
| 4 | **审计日志 append-only** | 3 天 | `admin_audit` 额外写 S3 Object Lock WORM 存储，管理员无法删改记录 |
| 5 | **`cargo fuzz` 关键路径** | 3 天 | `challenge::parse` / `token::decode` / siteverify JSON 解析持续 fuzz |
| 6 | **威胁模型文档** | 2 天 | `docs/THREAT_MODEL.md` 用 STRIDE 建模每个端点 |
| 7 | **`/api/v2/*`** | — | 如果有任何协议破坏性演化，走 v2 前缀；v1 保留至少 12 个月 |

---

## 可能取消的项目

| 项目 | 原因 |
|---|---|
| npm 单独发布 SDK | Portcullis 自带 manifest + 版本路径分发，主站从 `/sdk/manifest.json` 拉即可；重复 npm 包维护成本高 |
| React/Vue 包装组件 | `data-pow-captcha` auto-mount + `window.PowCaptcha.render()` 已经覆盖所有框架，无需专门包 |

---

## 不做的事（明确边界）

- **设备指纹 / 行为生物特征**：违反「不收集用户数据」承诺，永不引入
- **第三方打码平台对抗**：不可能根除，通过提高 `diff` + 缩短 `challenge_ttl_secs` 抬高经济成本即可
- **图像验证码 fallback**：重新引入依赖 + 体验降级，不在本项目范围
- **CAPTCHA-as-a-service 托管版**：本项目定位是自托管，不提供 SaaS

---

## 使用这份 roadmap

- PR / Issue 欢迎引用版本号（如 "Fixes v1.3.0 #3 参数范围校验"）
- 优先级顺序可根据使用方反馈调整
- v1.3.0 之后每个版本的启动时间取决于前一个版本的生产验证周期（通常 2-4 周）
