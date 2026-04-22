# PoW CAPTCHA 接入指南（AI Prompt）

> 本文档面向 AI 编程助手（Claude、GPT、Copilot 等），用于快速理解 PoW CAPTCHA 服务并生成接入代码。

---

## 你是什么

PoW CAPTCHA 是一个自托管的验证码服务，用工作量证明（Proof of Work）替代传统图像/滑块验证。用户点击后浏览器自动完成 Argon2id + SHA-256 挖矿计算，约 1-2 秒通过验证。

## 核心架构

```
浏览器 ──① POST /api/v1/challenge──► 验证服务 （发放挑战）
浏览器 ──② 本地 WASM 挖矿 ~1-2s──►  （无网络请求）
浏览器 ──③ POST /api/v1/verify────► 验证服务 （提交解答 → captcha_token）
浏览器 ──④ 表单携带 token────────► 业务后端
业务后端 ─⑤ POST /api/v1/siteverify─► 验证服务 （核验 token）
```

## 前端接入（零 JS 代码）

在 HTML 中加入以下 3 行即可：

```html
<script src="https://你的验证服务域名/sdk/pow-captcha.js"
        data-site-key="你的站点公钥"></script>

<div data-pow-captcha data-target="captcha_token"></div>
<input type="hidden" name="captcha_token" id="captcha_token" />
```

### 工作原理

- `<script>` 从验证服务加载 SDK（~11 KB），自动推导 API endpoint 和 WASM 路径
- `data-pow-captcha` 标记的 `<div>` 会自动渲染为验证码 widget
- 验证通过后 token 自动填入 `data-target` 指定的 `<input>`
- 无需写任何 JavaScript 代码

### data 属性参考

**`<script>` 上：**
| 属性 | 说明 |
|------|------|
| `data-site-key` | 站点公钥（必填，从管理面板获取） |
| `data-endpoint` | 覆盖 API 地址（默认从 src 自动推导） |
| `data-wasm-base` | 覆盖 WASM 路径（默认从 src 自动推导） |

**`<div data-pow-captcha>` 上：**
| 属性 | 说明 |
|------|------|
| `data-target` | 成功后自动填充的 input 的 id |
| `data-callback` | 全局回调函数名 |
| `data-theme` | `light`（默认） 或 `dark` |
| `data-lang` | `zh-CN`（默认） 或 `en-US` |

### 编程式 API（可选）

如果需要更灵活的控制：

```javascript
const widget = PowCaptcha.render('#container', {
  siteKey: '你的站点公钥',
  endpoint: 'https://你的验证服务域名',
  theme: 'light',
  lang: 'zh-CN',
  onSuccess: (token) => { /* token 可用 */ },
  onError: (err) => { /* 处理错误 */ },
  onExpired: () => { /* token 过期 */ },
});

widget.reset();              // 重置
widget.getResponse();        // 获取当前 token
widget.destroy();            // 销毁
```

---

## 后端接入

用户提交表单时，`captcha_token` 字段会随表单一起发送到你的业务后端。你的后端必须调用 `/api/v1/siteverify` 核验这个 token。

### API 端点

```
POST https://你的验证服务域名/api/v1/siteverify
Content-Type: application/json

{
  "token": "用户提交的 captcha_token",
  "secret_key": "你的站点私钥"
}
```

### 响应

成功：
```json
{ "success": true, "challenge_id": "uuid", "site_key": "pk_xxx" }
```

失败：
```json
{ "success": false, "error": "token 无效或已过期" }
```

### 关键规则

1. **必须在服务端校验**——不要信任前端的 `onSuccess` 回调
2. **`secret_key` 只在服务端使用**——绝不放到前端代码
3. **设置 3-5 秒超时**——避免验证服务故障拖累业务
4. **网络错误时拒绝请求**（fail-closed）
5. **token 单次有效**——同一 token 只能 siteverify 一次

### 各语言代码示例

#### Node.js（Express）

```javascript
app.post('/api/login', async (req, res) => {
  const { captcha_token } = req.body;
  
  const r = await fetch('https://captcha.example.com/api/v1/siteverify', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      token: captcha_token,
      secret_key: process.env.CAPTCHA_SECRET_KEY,
    }),
  });
  const data = await r.json();
  
  if (!data.success) {
    return res.status(403).json({ error: '验证码校验失败' });
  }
  
  // ... 正常业务逻辑
});
```

#### Python（FastAPI）

```python
import httpx
from fastapi import FastAPI, HTTPException

CAPTCHA_ENDPOINT = "https://captcha.example.com"
CAPTCHA_SECRET_KEY = os.environ["CAPTCHA_SECRET_KEY"]

@app.post("/api/login")
async def login(req: dict):
    async with httpx.AsyncClient(timeout=5) as c:
        r = await c.post(
            f"{CAPTCHA_ENDPOINT}/api/v1/siteverify",
            json={"token": req["captcha_token"], "secret_key": CAPTCHA_SECRET_KEY},
        )
    if not r.json().get("success"):
        raise HTTPException(403, "验证码校验失败")
```

#### Go

```go
func verifyCaptcha(token, endpoint, secretKey string) bool {
    body, _ := json.Marshal(map[string]string{
        "token": token, "secret_key": secretKey,
    })
    resp, err := http.Post(endpoint+"/api/v1/siteverify",
        "application/json", bytes.NewReader(body))
    if err != nil { return false }
    defer resp.Body.Close()
    var r struct{ Success bool }
    json.NewDecoder(resp.Body).Decode(&r)
    return r.Success
}
```

#### PHP

```php
function verifyCaptcha($token, $endpoint, $secretKey) {
    $ch = curl_init($endpoint . '/api/v1/siteverify');
    curl_setopt_array($ch, [
        CURLOPT_RETURNTRANSFER => true,
        CURLOPT_POST => true,
        CURLOPT_TIMEOUT => 5,
        CURLOPT_HTTPHEADER => ['Content-Type: application/json'],
        CURLOPT_POSTFIELDS => json_encode([
            'token' => $token, 'secret_key' => $secretKey
        ]),
    ]);
    $data = json_decode(curl_exec($ch), true);
    curl_close($ch);
    return $data['success'] ?? false;
}
```

#### curl

```bash
curl -X POST https://captcha.example.com/api/v1/siteverify \
  -H 'content-type: application/json' \
  -d '{"token":"<captcha_token>","secret_key":"<secret_key>"}'
```

---

## 完整 API 参考

| 端点 | 方法 | 说明 | 调用方 |
|------|------|------|--------|
| `/api/v1/challenge` | POST | 发放 PoW 挑战 | 浏览器 SDK |
| `/api/v1/verify` | POST | 提交解答换取 token | 浏览器 SDK |
| `/api/v1/verify/batch` | POST | 批量校验（最多 20 条） | 浏览器 SDK |
| `/api/v1/siteverify` | POST | 核验 token（业务后端调用） | 业务后端 |
| `/sdk/pow-captcha.js` | GET | 浏览器 SDK 脚本 | 浏览器 |
| `/sdk/captcha_wasm_bg.wasm` | GET | WASM 求解器 | 浏览器 SDK |
| `/admin/api/*` | * | 管理面板 API（需 Bearer Token） | 管理面板 |
| `/metrics` | GET | Prometheus 指标 | 监控系统 |
| `/healthz` | GET | 健康检查 | 运维 |

### `/api/v1/challenge` 请求/响应

```json
// 请求
{ "site_key": "pk_xxx" }

// 响应
{
  "success": true,
  "challenge": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "salt": "base64...",
    "diff": 18,
    "exp": 1737900000000,
    "site_key": "pk_xxx"
  },
  "sig": "base64..."
}
```

### `/api/v1/verify` 请求/响应

```json
// 请求
{ "challenge": { ... }, "sig": "base64...", "nonce": 184726 }

// 响应
{ "success": true, "captcha_token": "base64url.base64url", "exp": 1737900300000 }
```

### `/api/v1/siteverify` 请求/响应

```json
// 请求
{ "token": "<captcha_token>", "secret_key": "<secret_key>" }

// 成功响应
{ "success": true, "challenge_id": "uuid", "site_key": "pk_xxx" }

// 失败响应
{ "success": false, "error": "token 无效或已过期" }
```

---

## 部署信息

### Docker Compose（推荐）

```bash
# 1. 克隆仓库
git clone https://github.com/hor1zon777/Portcullis.git
cd Portcullis

# 2. 生成配置
cp .env.example .env
# 编辑 .env，用 openssl rand -hex 32 生成密钥

# 3. 启动
docker compose up -d
# → http://localhost/admin/    管理面板
# → http://localhost/api/...   API
# → http://localhost/sdk/...   SDK
```

### 本地开发

```bash
# 有 .env 文件后直接启动
cargo run -p captcha-server

# 管理面板开发
cd admin-ui && pnpm dev
```

### 环境变量

| 变量 | 必填 | 说明 |
|------|------|------|
| `CAPTCHA_SECRET` | 是 | HMAC 签名密钥（>= 32 字节） |
| `CAPTCHA_BIND` | 否 | 监听地址，默认 `0.0.0.0:8787` |
| `CAPTCHA_ADMIN_TOKEN` | 否 | 管理面板认证 Token |
| `CAPTCHA_DB_PATH` | 否 | SQLite 路径，默认 `data/captcha.db` |
| `CAPTCHA_SITES` | 否 | 初始站点 JSON（首次 seed 到 DB） |
| `CAPTCHA_CHALLENGE_TTL_SECS` | 否 | 挑战有效期，默认 120 |
| `CAPTCHA_TOKEN_TTL_SECS` | 否 | Token 有效期，默认 300 |

---

## 给 AI 的额外上下文

- 前端 SDK 是 IIFE 格式（`pow-captcha.js`），通过 `<script>` 加载后自动注册 `window.PowCaptcha`
- SDK 内部使用 WebAssembly 执行 Argon2id 计算，不依赖 Web Worker
- `captcha_token` 格式为 `base64url(payload).base64url(hmac_sig)`，5 分钟有效，单次使用
- 站点通过管理面板（`/admin/`）创建，密钥由服务端自动生成
- 难度参数 `diff` 控制前导零比特数：14 ≈ 0.2s，18 ≈ 1s，20 ≈ 3s（桌面端）
- 所有数据持久化在 SQLite（`data/captcha.db`）
- 仓库地址：https://github.com/hor1zon777/Portcullis
