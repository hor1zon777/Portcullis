# 接入指南

## 服务端部署

### 环境变量

| 变量 | 必填 | 默认 | 说明 |
|------|------|------|------|
| `CAPTCHA_SECRET` | 是 | — | HMAC 主密钥，≥ 32 字节 |
| `CAPTCHA_SECRET_PREVIOUS` | 否 | — | v1.5+ 旧主密钥；轮换期间用于验证未过期 token/stored_hash |
| `CAPTCHA_BIND` | 否 | `0.0.0.0:8787` | 监听地址 |
| `CAPTCHA_SITES` | 否 | `{}` | 站点配置 JSON（也可通过 TOML / 管理面板 CRUD） |
| `CAPTCHA_TOKEN_TTL_SECS` | 否 | `300` | captcha_token 有效期 |
| `CAPTCHA_CHALLENGE_TTL_SECS` | 否 | `120` | 挑战有效期 |
| `CAPTCHA_ADMIN_TOKEN` | 否 | — | 开启 `/admin/api/*` 的 Bearer Token，未设时管理面板禁用 |
| `CAPTCHA_MANIFEST_SIGNING_KEY` | 否 | — | Ed25519 manifest 签名私钥 seed（base64）；也可在面板「安全」页生成 |
| `CAPTCHA_ADMIN_WEBHOOK_URL` | 否 | — | v1.5+ 关键操作 webhook（Slack Incoming Webhook 兼容） |

### `CAPTCHA_SITES` 结构

```json
{
  "pk_test": {
    "secret_key": "sk_test_secret_at_least16",
    "diff": 18,
    "origins": ["https://example.com", "https://www.example.com"],
    "argon2_m_cost": 19456,
    "argon2_t_cost": 2,
    "argon2_p_cost": 1,
    "bind_token_to_ip": false,
    "bind_token_to_ua": false
  },
  "pk_mobile": {
    "secret_key": "sk_mobile_secret_16b",
    "diff": 14,
    "origins": ["https://m.example.com"]
  }
}
```

- `secret_key`（必填）：业务后端调用 `/siteverify` 时必须匹配，≥ 16 字符随机。服务端 **v1.5+ 启动时自动 HMAC 化**，env 输入的明文仅被使用一次；**管理员必须自行备份明文**，否则轮换 `CAPTCHA_SECRET` 或恢复业务后端配置时会失效
- `diff`（必填）：该站点的默认难度；可针对不同业务（评论/登录/注册）设置不同 `site_key`
- `origins`（可选）：CORS / Origin 白名单，留空放通全部
- `argon2_m_cost / t_cost / p_cost`（可选，v1.3+）：默认 `19456 / 2 / 1`（OWASP 2024 推荐），管理面板可视化编辑
- `bind_token_to_ip / bind_token_to_ua`（可选，v1.4+）：opt-in 身份绑定，默认关闭

> 推荐使用管理面板「站点」页管理站点，而非通过 `CAPTCHA_SITES` env 硬编码——管理面板会把创建结果（含一次性明文 `secret_key`）自动复制到剪贴板。

### 启动

```bash
cargo build --release -p captcha-server
CAPTCHA_SECRET=... CAPTCHA_ADMIN_TOKEN=... ./target/release/captcha-server
```

### Docker（示例）

```dockerfile
FROM rust:1.93-slim AS build
WORKDIR /app
COPY . .
RUN cargo build --release -p captcha-server

FROM debian:bookworm-slim
COPY --from=build /app/target/release/captcha-server /usr/local/bin/
EXPOSE 8787
CMD ["captcha-server"]
```

### 健康检查

```bash
curl http://localhost:8787/healthz
# -> "ok"
```

## 前端接入

### 方式 A：Auto-Mount（推荐，零 JS 代码）

服务端已内置 SDK 和 WASM 文件，访问 `/sdk/pow-captcha.js` 即可加载。

```html
<script src="https://captcha.example.com/sdk/pow-captcha.js"
        data-site-key="pk_test"></script>

<form>
  <div data-pow-captcha data-target="captcha_token"></div>
  <input type="hidden" name="captcha_token" id="captcha_token" />
  <button type="submit">提交</button>
</form>
```

**原理**：脚本自动从 `<script>` 的 `src` 推导 `endpoint` 和 WASM 路径，扫描所有 `[data-pow-captcha]` 元素并渲染 widget。验证成功后自动填充 `data-target` 指定的 input。

**data 属性**：

| 属性 | 位置 | 说明 |
|------|------|------|
| `data-site-key` | `<script>` 或 `<div>` | 站点公钥，必填 |
| `data-pow-captcha` | `<div>` | 标记容器元素 |
| `data-target` | `<div>` | 成功后自动填充的 input id |
| `data-theme` | `<div>` | `light`（默认）/ `dark` |
| `data-lang` | `<div>` | `zh-CN`（默认）/ `en-US` |
| `data-callback` | `<div>` | 全局回调函数名 |
| `data-endpoint` | `<script>` | 覆盖 endpoint（通常自动推导） |
| `data-wasm-base` | `<script>` | 覆盖 WASM 路径 |

### 方式 B：ES 模块（本地开发 / 自行构建）

```ts
// 从 sdk/src 直接导入（需 Vite / webpack 等构建工具）
import { render } from './sdk/src/index';

const widget = render('#captcha', {
  siteKey: 'pk_test',
  endpoint: 'https://captcha.example.com',
  lang: 'zh-CN',
  theme: 'light',
  onSuccess(token) {
    document.querySelector<HTMLInputElement>('#captcha_token')!.value = token;
  },
});
```

### 方式 C：全局 API

Auto-Mount 同时暴露 `window.PowCaptcha`：

```javascript
const widget = PowCaptcha.render('#my-captcha', {
  siteKey: 'pk_test',
  endpoint: 'https://captcha.example.com',
  onSuccess: (token) => { /* ... */ },
});
widget.reset();
widget.getResponse();
```

### 方式 D：带 SRI 的动态加载（推荐用于高敏感业务）

适用场景：主站 `captchaEndpoint` 在运行时可变（例如管理面板配置），无法在构建期写死 `<script integrity=...>`。

先拉 manifest、按清单里的 `integrity` 再注入 `<script>`：

```typescript
async function loadPortcullis(endpoint: string) {
  const ac = new AbortController();
  const t = setTimeout(() => ac.abort(), 3000);
  let manifest: any;
  try {
    manifest = await fetch(`${endpoint}/sdk/manifest.json`, {
      cache: 'no-store',
      signal: ac.signal,
    }).then((r) => r.json());
  } catch {
    // manifest 拉取失败 → 降级走旧路径（无 SRI）
    return injectScript(`${endpoint}/sdk/pow-captcha.js`, { crossOrigin: 'anonymous' });
  } finally {
    clearTimeout(t);
  }

  const sdk = manifest.artifacts['pow-captcha.js'];
  return injectScript(`${endpoint}${sdk.url}`, {
    integrity: sdk.integrity,
    crossOrigin: 'anonymous',
  });
}

function injectScript(src: string, attrs: Record<string, string> = {}) {
  return new Promise<void>((resolve, reject) => {
    const s = document.createElement('script');
    s.src = src;
    for (const [k, v] of Object.entries(attrs)) {
      s.setAttribute(k, v);
    }
    s.onload = () => resolve();
    s.onerror = () => reject(new Error('SDK 加载失败: ' + src));
    document.head.appendChild(s);
  });
}
```

**manifest 结构**：

```json
{
  "version": "1.5.0",
  "builtAt": 1745400000,
  "artifacts": {
    "pow-captcha.js":       { "url": "/sdk/v1.5.0/pow-captcha.js",       "integrity": "sha384-...", "size": 13554 },
    "captcha_wasm.js":      { "url": "/sdk/v1.5.0/captcha_wasm.js",      "integrity": "sha384-...", "size": ... },
    "captcha_wasm_bg.wasm": { "url": "/sdk/v1.5.0/captcha_wasm_bg.wasm", "integrity": "sha384-...", "size": ... }
  }
}
```

**注意事项**：
- 版本化路径 `/sdk/v{version}/*` 使用 `Cache-Control: immutable`，浏览器可长期缓存
- Portcullis 升级后旧版本字节从二进制消失，旧版本路径整体 404；主站应对 manifest 做短缓存（默认响应已设 `max-age=300`）
- manifest 可选带 Ed25519 签名头 `X-Portcullis-Signature`（v1.2+），在面板「安全」页一键启用/撤销；配套验签示例见 [`docs/TIER2_IMPLEMENTATION.md`](TIER2_IMPLEMENTATION.md)

### 配置项

| 字段 | 默认 | 说明 |
|------|------|------|
| `siteKey` | — | 站点公钥 |
| `endpoint` | — | 验证服务基地址 |
| `wasmBase` | `${endpoint}/pkg` | WASM 资源前缀（需托管 `captcha_wasm.js` 与 `captcha_wasm_bg.wasm`） |
| `theme` | `'light'` | `'light' \| 'dark'` |
| `lang` | `'zh-CN'` | `'zh-CN' \| 'en-US'` |
| `maxIters` | `10_000_000` | 最大尝试次数上限 |
| `onSuccess(token)` | — | 成功回调 |
| `onError(err)` | — | 失败回调 |
| `onExpired()` | — | token 过期回调 |

### WASM 资源托管

SDK 运行时需要两个资源：
- `captcha_wasm.js`（约 12 KB）
- `captcha_wasm_bg.wasm`（约 130 KB）

部署方式任选其一：
1. 和验证服务同源，通过静态文件托管
2. CDN（确保开启 CORS：`Access-Control-Allow-Origin: *`）
3. 自建 Nginx/OSS 目录

SDK 会按 `wasmBase` 加载。**必须保证同一版本的 WASM 与服务端 Argon2 参数一致**。

## 业务后端接入

### Node.js / Express

见 [`examples/backend-node/server.js`](../examples/backend-node/server.js) 或 [`docs/snippets/nodejs.md`](snippets/nodejs.md)。

```javascript
const r = await fetch('https://captcha.example.com/api/v1/siteverify', {
  method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({
    token: req.body.captcha_token,
    secret_key: process.env.CAPTCHA_SECRET_KEY,
    // v1.4+ 启用绑定后必须传；未启用也可传入，会被忽略
    client_ip: req.ip,                     // 需 app.set('trust proxy', true)
    user_agent: req.headers['user-agent'],
  }),
});
if (!(await r.json()).success) return res.status(403).end();
```

### Python / FastAPI

```python
import httpx
from fastapi import FastAPI, HTTPException, Request

app = FastAPI()
CAPTCHA_ENDPOINT = "https://captcha.example.com"
CAPTCHA_SECRET_KEY = "sk_test_secret_at_least16"

@app.post("/login")
async def login(req: dict, request: Request):
    async with httpx.AsyncClient(timeout=5.0) as c:
        r = await c.post(
            f"{CAPTCHA_ENDPOINT}/api/v1/siteverify",
            json={
                "token": req["captcha_token"],
                "secret_key": CAPTCHA_SECRET_KEY,
                # v1.4+ 身份绑定
                "client_ip": request.client.host,
                "user_agent": request.headers.get("user-agent", ""),
            },
        )
    if not r.json().get("success"):
        raise HTTPException(400, "验证码校验失败")
    # ... 业务逻辑
```

### siteverify 请求字段（v1.4+）

| 字段 | 必填 | 说明 |
|------|------|------|
| `token` | 是 | 从前端收到的 captcha_token |
| `secret_key` | 是 | 对应站点的明文 secret_key（业务方持有，服务端 HMAC 比对） |
| `client_ip` | 条件 | 启用 `bind_token_to_ip` 的 site **必填**；未启用时被忽略 |
| `user_agent` | 条件 | 启用 `bind_token_to_ua` 的 site **必填**；未启用时被忽略 |

### siteverify 响应

```json
{"success": true, "challenge_id": "...", "site_key": "pk_test"}
```

或失败：

```json
{"success": false, "error": "token 无效或已过期"}
```

常见失败 `error`：
- `token 无效或已过期`：签名不匹配 / payload 过期（TTL 到）
- `site_key 已下线`：对应 site 已在管理面板删除
- `secret_key 不匹配`：业务方持有的明文与服务端 HMAC 不一致
- `token 已被核验过（单次使用）`：重复核验，防重放
- `token 要求 IP 绑定，但 siteverify 未携带 client_ip` / `client_ip 与 token 绑定不一致`（v1.4+）
- `token 要求 UA 绑定，但 siteverify 未携带 user_agent`（v1.4+）

### 通用要点

1. **不要信任浏览器传来的任何"验证已通过"标记**；必须走 `/siteverify`
2. **单次使用**：token 被核验后服务端已在 SQLite 与内存 store 双写防重放；业务侧如有集群也应对 `challenge_id` 做幂等
3. **绑定会话**：可将 token 校验结果与 session/userId 绑定，防止跨会话复用
4. **secret_key 保管**：v1.5+ 服务端只保留 HMAC，业务后端持有明文；明文在管理面板创建站点时**一次性返回**并自动复制到剪贴板，**错过即无法找回**——必须同时存入 Vault / 密码管理器
5. **失败 fail-closed**：`/siteverify` 超时或网络错误应拒绝业务请求，不要放行

## 本地开发

```bash
# 一个终端：启动验证服务
export CAPTCHA_SECRET="this-is-a-dev-secret-must-be-32-bytes+"
export CAPTCHA_SITES='{"pk_test":{"secret_key":"sk_test_secret_at_least16","diff":12,"origins":["http://localhost:5173"]}}'
cargo run -p captcha-server

# 另一个终端：启动 SDK 开发服务器
cd sdk && pnpm dev
```

打开 `http://localhost:5173` 即可看到 widget。

## 升级算法参数（v1.3+ 可运行时调整）

自 v1.3.0 起 Argon2 参数每 challenge 下发并经 HMAC 签名保护，**无需重建 WASM**：

1. 管理面板「站点」页编辑对应 site，调整 `m_cost` / `t_cost`
2. 保存即热生效，新发出的 challenge 按新参数；旧 challenge（签名覆盖原参数）仍可完成
3. 范围：`m_cost ∈ [8, 65536]`（KiB）、`t_cost ∈ [1, 10]`、`p_cost` 固定为 1

> v1.2.x 及之前的部署必须先升级到 v1.3+，再使用运行时调参能力。升级步骤见 [UPGRADING.md](UPGRADING.md)。

## 密钥轮换（v1.5+）

### `CAPTCHA_SECRET` 主密钥轮换

```bash
# 1. 保留当前 secret 到 SECRET_PREVIOUS
export CAPTCHA_SECRET_PREVIOUS="$CAPTCHA_SECRET"
# 2. 生成新 secret
export CAPTCHA_SECRET="$(./captcha-server gen-secret)"
# 3. 重启服务：新 token/challenge 用 current 签，旧的仍可通过 previous 验证
# 4. 等待 token_ttl_secs 窗口（默认 300s）后重新生成所有站点 secret_key（管理面板「站点」→「编辑」→ 保存）
# 5. 确认业务方后端已更新新的 secret_key
# 6. 移除 CAPTCHA_SECRET_PREVIOUS，重启
```

### 站点 `secret_key` 轮换

在管理面板「站点」页点击「编辑」 → 重新保存即可生成新的 secret_key（v1.5+ 规划，当前需要删除并重建站点）。

## 审计与 webhook（v1.5+）

- 管理面板「审计」页查看所有 admin 操作记录：站点 CRUD、IP 封解、密钥生成撤销、登录失败
- 配置 `CAPTCHA_ADMIN_WEBHOOK_URL` 接收 Slack Incoming Webhook 兼容的通知
- admin 登录连续 30 次失败 → IP 被 ban 15 分钟（HTTP 429）
