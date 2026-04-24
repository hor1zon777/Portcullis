# API 稳定性承诺

## v1.0.0 起 `/api/v1/*` 接口格式冻结

以下端点的请求体和响应体格式在 v1.x 系列中保持向后兼容：

| 端点 | 状态 | 备注 |
|------|------|------|
| `POST /api/v1/challenge` | 冻结 | 响应 `challenge` 对象可能新增字段（v1.3 加了 `m_cost` / `t_cost` / `p_cost`） |
| `POST /api/v1/verify` | 冻结 | 请求体照原样回传 challenge，服务端会解析新字段 |
| `POST /api/v1/verify/batch` | 冻结 | — |
| `POST /api/v1/siteverify` | 冻结 | 请求体允许新增 optional 字段（v1.4 加了 `client_ip` / `user_agent`） |
| `GET /healthz` | 冻结 | — |
| `GET /metrics` | 冻结 | Prometheus 格式，指标名称冻结 |
| `GET /sdk/*` | 冻结 | 文件路由冻结，内容随版本更新 |
| `GET /sdk/manifest.json` | 冻结（v1.2+） | 结构冻结；可选 `X-Portcullis-Signature` 响应头 |
| `GET /sdk/v{version}/*` | 冻结（v1.2+） | 版本化 immutable 路径 |
| `/admin/api/*` | **非正式契约** | 管理 API 与管理面板配套演进，可能随版本扩展/改动 |

## 兼容性规则

### 不破坏兼容（v1.x 内允许）
- 响应体新增 optional 字段（消费方应忽略未知字段）
- 请求体新增 optional 字段（未启用相关特性时会被忽略）
- 新增 HTTP 端点
- 新增查询参数（不影响现有请求）
- 新增 Prometheus 指标
- 新增 Error `error` 字段取值
- 性能优化 / bug 修复

### 破坏兼容（必须升至 v2.0.0）
- 删除或重命名现有字段
- 修改字段类型
- 修改 HTTP 状态码语义
- 删除端点
- 修改 `/api/v1/*` 请求体的**必填**字段列表

## 已发生的兼容性演进（v1.0 → v1.5）

| 版本 | 新增 | 向后兼容性 |
|------|------|-----------|
| v1.2 | `GET /sdk/manifest.json` / `GET /sdk/v{version}/*` / `X-Portcullis-Signature` 响应头 | 新增端点，旧 `/sdk/*` 路径保留 |
| v1.3 | `challenge.m_cost` / `t_cost` / `p_cost` 字段；签名字节序列扩展 | 旧客户端 JSON 无新字段时 `serde(default)` 回填 legacy `4096/1/1`；新老服务端可并存（但新服务端发给旧 SDK 会失败）|
| v1.4 | `token.payload.ip_hash` / `ua_hash`（opt-in）；`/api/v1/siteverify` 请求体新增 `client_ip` / `user_agent` | 未启用绑定的 site 完全向后兼容 |
| v1.5 | `/admin/api/audit` 端点；`SiteView.secret_key` 返回 `"(hashed)"` 占位；`secret_key_hashed` 字段；**服务端内部 `secret_key` 改 HMAC 存储** | `/api/v1/*` 外部 API 完全兼容；**`secret_key` hash 化是单向 DB 迁移**，升级前必须备份原文 |

## SDK 版本

浏览器端 WASM 通过 `version()` 函数返回版本号。
自 v1.2+ 起推荐使用 `/sdk/manifest.json` 加载，通过 `<script integrity=...>` 固化版本。

```html
<!-- v1.2+ 推荐 -->
<script src="https://captcha.example.com/sdk/v1.5.0/pow-captcha.js"
        integrity="sha384-..."
        crossorigin="anonymous"></script>

<!-- 向后兼容路径（无 SRI） -->
<script src="https://captcha.example.com/sdk/pow-captcha.js"></script>
```

## 服务端升级兼容性

- **v1.x → v1.y**：DB schema 仅做增量迁移（`ALTER TABLE ADD COLUMN ...`），旧二进制忽略新增列，**可双向回滚**（v1.4 → v1.5 的 `secret_key` hash 化例外，见下）
- **v1.5 的 `secret_key` hash 迁移不可逆**：升级前必须备份所有站点 `secret_key` 原文。若未备份且需要回滚，只能**删除所有站点重建**
- **`CAPTCHA_SECRET` 轮换**：v1.5+ 通过 `CAPTCHA_SECRET_PREVIOUS` 支持无缝双 key 过渡；直接替换会让所有 stored_hash 失效，详见 [UPGRADING.md](UPGRADING.md)

## SDK 与服务端版本关系

- **`/api/v1/*` 向后兼容**：新版本服务端接受旧版本 SDK 发来的请求（新字段 `serde(default)` 回填）
- **旧 SDK + 新服务端 Argon2 参数不匹配**：v1.3+ 后旧 SDK 的硬编码 Argon2 参数可能与新默认值不同，导致 base_hash 不一致 → verify 失败。因此**服务端和 SDK 必须同步升级**到 v1.3+
- **建议部署方式**：Docker Compose（SDK 随 `captcha-server` 镜像原子分发）或 CI 流水线同时更新服务端 + 主站缓存清理

## v2.0 的破坏性变更（规划中）

> 预计工作量远超过 v1.x 补丁；见 [ROADMAP.md](ROADMAP.md)。v2.0 至少保留 `/api/v1/*` 可用 12 个月，任何 v1→v2 的破坏改动会通过 `/api/v2/*` 前缀引入。

可能的 v2 变化：
- Redis 存储后端（需要协议内显式 store 抽象）
- RBAC + JWT admin 登录
- KMS/HSM 集成（`CAPTCHA_SECRET` 从 AWS KMS / Vault 加载）
- 协议级签名算法演进（若 HMAC-SHA256 不足以应对量子威胁）
