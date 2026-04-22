# 安全加固与威胁模型

## 威胁模型

| # | 威胁 | 攻击动机 | 本方案对策 |
|---|------|---------|-----------|
| 1 | **重放攻击**：截获一次合法 nonce，反复提交 verify | 绕过验证成本 | verify 成功后将 `challenge.id` 加入黑名单，TTL=`challenge.exp` |
| 2 | **伪造挑战**：构造 `diff=0` 的假挑战骗过服务端 | 一秒破万次验证 | HMAC-SHA256 签名；验证时常数时间比较 |
| 3 | **预计算/彩虹表** | 离线穷举 salt | 每个 challenge 使用 16 字节加密随机盐；Argon2id 本身内存硬化 |
| 4 | **GPU / FPGA 农场** | 并发解题 | Argon2id m=4096 KiB，每个 nonce 独占 4 MiB 内存；相比 SHA-256，GPU 单卡并发数下降 1~2 个数量级 |
| 5 | **第三方打码外包** | 用人工/GPU 集群代解 | 不可根除；通过缩短 `challenge.exp`（默认 120s）+ 动态拉高 `diff` 提升经济成本 |
| 6 | **跨站盗用 SDK** | A 站的 siteKey 用于 B 站 | CORS 白名单 + Referer 校验 + `site_key` 与 origin 绑定（v2 强制） |
| 7 | **captcha_token 滥用** | 一次验证多次复用 | 单次使用建议（业务侧缓存） + 5 分钟过期 + 绑定 `site_key` |
| 8 | **时序攻击** | 通过 HMAC 比较耗时差异推导密钥 | `subtle::ConstantTimeEq` 常数时间比较 |
| 9 | **服务端 CPU 压制** | 恶意批量提交 verify，迫使服务端执行 Argon2 | tower-governor 按 IP 限流；防重放检测先于 Argon2 计算 |
| 10 | **DoS / 挑战洪水** | 批量申请 challenge 消耗内存 | challenge 是无状态的（未写入存储），不消耗内存；限流按 IP |
| 11 | **中间人篡改** | 途中修改 `diff=0` | HTTPS 强制 + HMAC 签名保护所有字段 |
| 12 | **客户端/服务端算法漂移** | 客户端算对的 nonce 服务端验不过 | 客户端 WASM 与服务端共享同一份 `captcha-core` Rust 代码，编译期一致 |

## 配置加固清单

- [ ] `CAPTCHA_SECRET` 至少 32 字节，随机生成（`openssl rand -hex 32`）
- [ ] 所有 `secret_key` 至少 16 字符随机，分站点独立
- [ ] 生产环境 HTTPS 终止在反向代理（Nginx / Caddy）
- [ ] 反向代理开启 IP 级限流（`10 req/min` 建议）
- [ ] 若使用反向代理，通过 `X-Forwarded-For` 传递真实 IP；本服务 v2 会接入 IP 限流
- [ ] 日志脱敏：不打印 `CAPTCHA_SECRET`、`secret_key`、`captcha_token` 的明文
- [ ] `CAPTCHA_SITES` 通过 secret manager（Vault / AWS Secrets Manager）注入，不入代码库
- [ ] 进程级资源限制：cgroup / systemd 限制 CPU、RAM
- [ ] 监控告警：
  - verify 拒绝率异常升高（可能为解题能力下降或攻击）
  - challenge/verify 比例严重失衡（可能为自动化脚本）
  - `429` / `409` 激增

## 难度选择建议

| 场景 | diff | 理由 |
|------|------|------|
| 评论 / 点赞 | 12~14 | 低风险，容忍 1~2 秒等待 |
| 登录 | 16~18 | 中风险，用户心理预期 3~5 秒 |
| 注册 / 找回密码 | 20~22 | 高风险，容忍 10~30 秒 |
| 提币 / 大额操作 | 叠加二次认证，不单靠 PoW | PoW 非授权机制 |

## 合规与隐私

- 本方案**不收集**用户生物特征、行为轨迹、设备指纹
- 不向第三方发起任何网络请求（与 reCAPTCHA / hCaptcha 相比）
- 服务端仅存储：`challenge.id → exp`（内存，TTL 后清理）
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
| Admin Token 保密 | `[admin].token` 与 `CAPTCHA_SECRET` 同等级别，不入 Git |
| 网络隔离 | 生产环境中 `/admin/*` 应通过 Nginx `allow/deny` 限制内网访问 |
| HTTPS | 管理面板登录传输 Token，必须走 HTTPS |
| Token 轮换 | 定期更换 `[admin].token`，修改 TOML 30 秒内自动热重载 |

## 后续改进方向

- Redis 存储后端（多实例共享防重放）
- E2E 测试（Playwright）
- Fuzz 测试（cargo-fuzz）
