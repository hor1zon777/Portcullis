# 安全加固与威胁模型

## 威胁模型

| # | 威胁 | 攻击动机 | 本方案对策 |
|---|------|---------|-----------|
| 1 | **重放攻击**：截获一次合法 nonce，反复提交 verify | 绕过验证成本 | verify 成功后将 `challenge.id` 加入黑名单（memory store + SQLite 双写），TTL=`challenge.exp` |
| 2 | **伪造挑战**：构造 `diff=0` 的假挑战骗过服务端 | 一秒破万次验证 | HMAC-SHA256 签名；签名字节序列覆盖所有字段（含 v1.3+ 的 Argon2 参数） |
| 3 | **预计算/彩虹表** | 离线穷举 salt | 每个 challenge 使用 16 字节加密随机盐；Argon2id 本身内存硬化 |
| 4 | **GPU / FPGA 农场** | 并发解题 | Argon2id 默认 m=19456 KiB（v1.3+，OWASP 2024 推荐档），每个 nonce 独占 19 MiB 内存；相比 SHA-256，GPU 单卡并发数下降 1~2 个数量级 |
| 5 | **WASM 版本信息泄漏** | 从二进制 `argon2-0.5.3/src/params.rs` 字符串推断算法参数 | v1.3+ 参数每 challenge 下发 + HMAC 签名保护，攻击者即使知道版本也无法预计算（每站点参数不同、可随时调） |
| 6 | **第三方打码外包** | 用人工/GPU 集群代解后分发 token 到多机器 | v1.4+ opt-in 绑定 client IP / User-Agent 到 token；siteverify 强制比对 |
| 7 | **跨站盗用 SDK** | A 站的 siteKey 用于 B 站 | CORS + Origin 白名单；`site_key` 与 origin 绑定 |
| 8 | **captcha_token 滥用** | 一次验证多次复用 | 单次使用：`challenge_id` 核验后写入 `replay_nonces` 表；5 分钟过期；绑定 `site_key` |
| 9 | **时序攻击** | 通过 HMAC 比较耗时差异推导密钥 | `subtle::ConstantTimeEq` 常数时间比较；`verify_sig_any` 用 `\|=` 累积避免多 key 时序差 |
| 10 | **服务端 CPU 压制** | 恶意批量提交 verify，迫使服务端执行 Argon2 | `governor` 按 IP 令牌桶限流（5 req/s burst 20）；防重放检测先于 Argon2 计算；签名校验先于 PoW |
| 11 | **DoS / 挑战洪水** | 批量申请 challenge 消耗内存 | challenge 无状态（仅 salt + 元数据，不占存储）；限流按 IP |
| 12 | **中间人篡改** | 途中修改 `diff=0` / `m_cost=8` | HTTPS + HMAC 签名保护所有字段（含 v1.3+ Argon2 参数） |
| 13 | **客户端/服务端算法漂移** | 客户端算对的 nonce 服务端验不过 | 客户端 WASM 与服务端共享同一份 `captcha-core` Rust 代码；v1.3+ Argon2 参数由服务端 challenge 下发，客户端按 challenge 构建实例 |
| 14 | **DB 泄漏 → 所有站点 secret_key 泄漏** | SQLite 备份外泄 | v1.5+ 改存 `HMAC-SHA256(CAPTCHA_SECRET, plain)`，DB 中只有 hash；业务方持有明文，服务端 HMAC 再比 |
| 15 | **Admin token 暴力破解** | 字典/GPU 尝试 admin token | v1.5+ 连续 30 次失败触发 15 分钟 IP ban（HTTP 429）；所有失败写入审计表 |
| 16 | **内部操作无痕修改** | 恶意 admin 修改站点后抹迹 | v1.5+ `admin_audit` 表记录所有操作（CRUD / IP 封解 / 密钥操作 / 登录失败）；token 前缀脱敏追踪 |
| 17 | **主密钥一次性失效** | `CAPTCHA_SECRET` 泄漏需紧急更换 | v1.5+ 双 key 轮换：`secret_previous` 期间两把 key 都接受，待 token TTL 过期后删除旧 key |

## 配置加固清单

### 基础（v1.0）
- [ ] `CAPTCHA_SECRET` 至少 32 字节，随机生成（`openssl rand -hex 32` 或 `./captcha-server gen-secret`）
- [ ] 所有 `secret_key` 至少 16 字符随机，分站点独立
- [ ] 生产环境 HTTPS 终止在反向代理（Nginx / Caddy）
- [ ] 反向代理开启 IP 级限流（`10 req/min` 建议）
- [ ] 反向代理正确透传 `X-Forwarded-For` / `X-Real-IP`（见 `docs/DEPLOY.md` §7.1）
- [ ] 日志脱敏：不打印 `CAPTCHA_SECRET`、`secret_key`、`captcha_token` 的明文
- [ ] `CAPTCHA_SITES` 通过 secret manager（Vault / AWS Secrets Manager）注入，不入代码库
- [ ] 进程级资源限制：cgroup / systemd 限制 CPU、RAM

### SDK 分发（v1.2+）
- [ ] 主站使用 `/sdk/manifest.json` + SRI `integrity` 加载 SDK（[方式 D](INTEGRATION.md#方式-d带-sri-的动态加载推荐用于高敏感业务)）
- [ ] 在管理面板「安全」页一键启用 Ed25519 manifest 签名；主站侧校验 `X-Portcullis-Signature`

### Argon2 参数（v1.3+）
- [ ] 默认参数 `19456 / 2 / 1` 已符合 OWASP 2024；高风险站点可提升到 `32768 / 3 / 1`
- [ ] 管理面板「站点」页按业务分别调参；低频高价值场景拉高 `m_cost`

### Token 身份绑定（v1.4+，opt-in）
- [ ] 评估隐私权衡后，对**注册 / 支付确认 / 大额提现**等启用 `bind_token_to_ip`
- [ ] `bind_token_to_ua` 仅在短 TTL 下启用（推荐 `token_ttl_secs ≤ 120`），避免浏览器自动升级导致 UA 漂移
- [ ] 业务后端 siteverify 同步传入 `client_ip` / `user_agent`（见 `docs/INTEGRATION.md`）

### 服务端密钥管理（v1.5+）
- [ ] 升级到 v1.5 前**必须**备份所有站点的 `secret_key` 原文到密码管理器——升级后 DB 里只剩 HMAC 不可逆
- [ ] 新建站点时在管理面板**立即**保存创建响应返回的明文（UI 已自动复制到剪贴板 + 15 秒 toast 提示）
- [ ] 周期轮换 `CAPTCHA_SECRET`（每 90 天建议）：`SECRET_PREVIOUS = 旧` → 重启 → 等 token TTL → 重建站点 secret_key → 清除 `SECRET_PREVIOUS`
- [ ] 配置 `CAPTCHA_ADMIN_WEBHOOK_URL` 接收关键操作通知
- [ ] 定期查阅 `/admin/audit` 审计页；关注 `login.fail` 频次与 `manifest.revoke` 异常

## 难度选择建议

| 场景 | diff | Argon2 m_cost | 理由 |
|------|------|---------------|------|
| 评论 / 点赞 | 12~14 | 4096~8192 | 低风险，容忍 1~2 秒等待 |
| 登录 | 16~18 | 19456（默认） | 中风险，用户心理预期 2~5 秒 |
| 注册 / 找回密码 | 18~20 | 19456~32768 | 高风险，容忍 5~15 秒 |
| 提币 / 大额操作 | 20~22 + bind_token_to_ip | 32768 | 高风险，叠加二次认证，PoW 非授权机制 |

## 合规与隐私

- 本方案**不收集**用户生物特征、行为轨迹、设备指纹
- 不向第三方发起任何网络请求（与 reCAPTCHA / hCaptcha 相比）
- 服务端仅存储：
  - `sites`（站点配置，`secret_key` v1.5+ 为 HMAC）
  - `ip_lists`（IP 黑白名单）
  - `request_log`（最近请求日志，按 `retention_days` 过期清理）
  - `replay_nonces`（防重放 ID，TTL 后清理）
  - `server_secrets`（manifest 签名私钥 seed）
  - `admin_audit`（v1.5+ 管理员操作记录）
- v1.4+ opt-in 启用绑定时，`ip_hash` / `ua_hash` **仅进 token**，不写 DB、不写日志；token 到期自动失效
- 无需 Cookie，不跨站追踪，符合 GDPR / CCPA

## 不适用场景

本方案不能替代以下机制：

- **账号盗用防护**：需要行为风控 + 二次验证（短信/TOTP）
- **支付欺诈检测**：需要风控模型 + 人工审核
- **DDoS 缓解**：需要网络层清洗（CDN / WAF）
- **API 滥用**：需要配额 + 计费

PoW CAPTCHA 仅作为「机器人 vs 人类」的第一道闸门，增加自动化攻击的单次成本。

## 管理面板安全

| 要求 | 说明 |
|------|------|
| Admin Token 保密 | `[admin].token` 与 `CAPTCHA_SECRET` 同等级别，不入 Git；建议 32+ 字符随机 |
| 登录失败 ban | v1.5+ 自动：连续 30 次失败 → IP ban 15 分钟 + 429 响应 + 全量审计 |
| 网络隔离 | 生产环境中 `/admin/*` 应通过 Nginx `allow/deny` 限制内网访问 |
| HTTPS | 管理面板登录传输 Token，必须走 HTTPS |
| Token 轮换 | 定期更换 `[admin].token`，修改 TOML 30 秒内自动热重载 |
| 审计定期复盘 | v1.5+ 每周查阅 `/admin/audit`，关注未知 IP 的 login.fail 或密钥操作 |
| Webhook 告警 | v1.5+ 配置 `admin_webhook_url` 接收 Slack 通知，关键操作即时告警 |

## 后续改进方向（[ROADMAP.md](ROADMAP.md)）

- v1.6：供应链签名（cosign / SBOM / cargo-audit）
- v2.0：Redis 存储后端、RBAC、KMS 抽象、`cargo fuzz`、威胁模型 STRIDE 文档
