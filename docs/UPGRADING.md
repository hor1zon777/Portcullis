# 升级指南

> 最新版本：**v1.3.0**（PoW 参数下发化）

PoW CAPTCHA 的核心算法由客户端 WASM 与服务端共同执行，因此在 v1.2.x 之前**客户端与服务端必须编译自同一套 Argon2 参数**。v1.3.0 起，参数从 `challenge` 下发并由 HMAC 签名覆盖，客户端只需根据 challenge 内容动态构建 Argon2 实例，升级耦合大幅降低。

---

## v1.2.x → v1.3.0

### 1. 核心改动

| 变化 | 含义 |
|------|------|
| `Challenge` 新增 `m_cost / t_cost / p_cost` 字段 | Argon2 参数逐 challenge 下发；纳入 `to_sign_bytes()`，HMAC 签名覆盖，篡改即拒 |
| 全局 `OnceLock<Argon2>` 移除 | 服务端按 challenge 参数构建 Argon2 实例 |
| 默认参数提档 | `4096/1/1` → `19456/2/1`（OWASP 2024 Argon2id 第二档） |
| `SiteConfig` 新增 `argon2_m_cost/t_cost/p_cost` | 每站点可独立覆盖 |
| DB schema 增量迁移 | 启动时 `ALTER TABLE sites ADD COLUMN …`（幂等） |
| 参数范围校验 | `m_cost ∈ [8, 65536]`、`t_cost ∈ [1, 10]`、`p_cost == 1` |

### 2. 向后兼容性

- **旧客户端（v1.2.x SDK）** 仍可访问 v1.3.0 服务端：旧 SDK 不发 `m_cost` 等字段，服务端 challenge 响应会包含新字段；旧 SDK 用 `serde(default)` 解析兜底，仍按硬编码 `4096/1/1` 求解，导致 **base_hash 与服务端不一致 → 验证必然失败**。因此 v1.2.x SDK 必须随服务端同步升级到 v1.3.0；若主站靠 `/sdk/manifest.json` 热加载 SDK（默认场景），Docker 镜像或新二进制上线即完成升级。
- **旧格式 JSON**（`challenge` 字段无 `m_cost/t_cost/p_cost`）反序列化时自动回填 `LEGACY_M_COST=4096 / LEGACY_T_COST=1 / LEGACY_P_COST=1`，方便调试脚本或第三方 CLI。
- **外部 API**（`/api/v1/*`）格式向后兼容：challenge 响应新增字段，不破坏既有主站后端。

### 3. 升级步骤（推荐：蓝绿/灰度）

#### Docker Compose（一键部署场景）

```bash
# 拉取新镜像
docker compose pull captcha-server admin-ui nginx

# 原子替换
docker compose up -d
```

服务端 + SDK + 管理面板同步上线。`/sdk/manifest.json` 会在 10 秒内被主站缓存失效策略拉到新版本，随后主站 `<script integrity=…>` 自动加载新 SDK。

#### 二进制部署

```bash
# 1. 停机窗口内升级
systemctl stop captcha-server
cp captcha-server.new /usr/local/bin/captcha-server
systemctl start captcha-server
```

启动时 `migrate()` 幂等执行 `ALTER TABLE sites ADD COLUMN argon2_m_cost/t_cost/p_cost`，已有站点自动填充默认值 `19456/2/1`（与新版本一致）。

#### 蓝绿发布（零停机）

1. 新版本部署到备用节点 A'
2. LB 切 10% 流量到 A'，观察管理面板 `/admin/api/logs` 无失败率异常
3. 逐步切到 100%
4. 等待 `token_ttl_secs`（默认 300s）让旧 token 过期
5. 下线旧节点

### 4. 参数调整（运行时）

升级后可随时通过管理面板调整参数，**无需重启**。

- 管理面板「站点管理」页，每个站点新增两列 `m_cost / t_cost`，点击「编辑」直接改；
- 或 `PUT /admin/api/sites/<key>` 携带 `argon2_m_cost` / `argon2_t_cost`：

  ```bash
  curl -X PUT https://captcha.example.com/admin/api/sites/pk_abc \
    -H "Authorization: Bearer $ADMIN_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"argon2_m_cost": 32768, "argon2_t_cost": 3}'
  ```

- 改动立即生效，新发出的 challenge 使用新参数；旧 challenge（签名覆盖原参数）继续按发放时的参数验证，互不影响。
- 管理面板输入框超出 `[8, 65536]` 或 `[1, 10]` 范围会被服务端 `validate_argon2_params` 拒绝并返回 `400`。

### 5. 回滚到 v1.2.x

v1.3.0 DB schema 向后兼容 v1.2.x（新增列被旧二进制忽略）。

```bash
docker compose pull captcha-server:1.2.5
docker compose up -d captcha-server
```

- v1.2.x 仍能读取 `sites` 表，忽略多出的 3 列；
- 旧 server 发出的 challenge 不含 `m_cost` 等字段，新 SDK 用 `serde(default)` 回填 4096/1/1，与 v1.2.x 硬编码一致，不会破坏。
- **注意**：如果管理员在 v1.3.0 期间把某站点 `argon2_m_cost` 改成了非 4096 的值，回滚后由于 v1.2.x 硬编码，该站点的 challenge 与 SDK 行为仍是 4096/1/1（DB 中的新列 v1.2.x 代码不读）。回滚前建议确认所有站点保持默认参数，避免行为跳变。

### 6. 性能参考

桌面 Chrome（M1 MacBook，v130）首次 Argon2 base_hash 计算耗时：

| 参数 `(m,t,p)` | 单次 Argon2 | 总求解（含 SHA-256 循环，diff=18）| 适用场景 |
|-----|-------|----------|----------|
| `4096 / 1 / 1`   | ~5 ms  | ~1.8 s  | v1.2.x 默认；低摩擦 |
| `19456 / 2 / 1`  | ~20 ms | ~2.0 s  | **v1.3.0 默认**，OWASP 推荐 |
| `32768 / 3 / 1`  | ~80 ms | ~2.3 s  | 高风险站点 |
| `65536 / 4 / 1`  | ~200 ms | ~2.8 s | 极端场景（慎用） |

> 服务端每次验证同样跑一次 Argon2，建议 `m_cost` 上限考虑自身容量。`19456 KiB ≈ 20 MiB` × 并发 → 需确认服务器物理内存充足。

---

## 历史升级路径

### v1.2.x 内部升级

v1.2.0 → v1.2.5 全部是 SDK 加固、前端与构建期脱敏，不涉及协议或 DB schema 变化。直接替换二进制 / 镜像即可。

### v1.1.x → v1.2.x

新增「安全」页、`manifest_signing_key` 配置与 `server_secrets` 表。`migrate()` 幂等创建表；旧 env/toml 密钥在首次启动时自动导入 DB。

### v1.0.x → v1.1.x

- 新增 `[admin]` 配置段和 `CAPTCHA_ADMIN_TOKEN` 环境变量
- Docker Compose 切三服务架构（`captcha-server` / `admin-ui` / `nginx`）

### v1.0.0 及之前

见各版本 CHANGELOG；v0.x → v1.0.0 仅冻结 `/api/v1/*` 接口格式，代码结构无破坏性变化。

---

## `diff` 调整（所有版本通用）

`diff` 参数不影响算法本身，只影响难度判定。通过配置即可调整：

```toml
# captcha.toml
[[sites]]
key = "pk_example"
diff = 20  # 从 18 改为 20
```

保存后 30 秒内自动热重载生效，无需重启。管理面板「站点管理」页直接编辑 Diff 字段效果等价，但会即时同步到 DB + 内存。

---

## API 版本兼容

- `/api/v1/*` 请求/响应格式在 v1.0.0 冻结
- 新增字段向后兼容（例如 v1.3.0 给 `challenge` 加 `m_cost / t_cost / p_cost`）
- 删除/改名字段会通过 `/api/v2/*` 引入，v1 将保留至少 12 个月
- SDK `version()` 函数返回 WASM 版本号，可用于前端自检
