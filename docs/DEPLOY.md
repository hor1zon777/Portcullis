# 部署教程

从零开始部署 PoW CAPTCHA 验证码服务，涵盖本地开发、单二进制、Docker 三种方式。

---

## 目录

- [前置要求](#前置要求)
- [一、从源码构建](#一从源码构建)
- [二、配置文件](#二配置文件)
- [三、启动服务](#三启动服务)
- [四、前端接入](#四前端接入)
- [五、业务后端接入](#五业务后端接入)
- [六、Docker 部署](#六docker-部署)
- [七、反向代理（Nginx）](#七反向代理nginx)
- [八、验证部署](#八验证部署)
- [九、监控与告警](#九监控与告警)
- [十、常见问题](#十常见问题)

---

## 前置要求

### 从源码构建需要

| 工具 | 最低版本 | 安装方式 |
|------|---------|---------|
| Rust | 1.70+ | [rustup.rs](https://rustup.rs/) |
| wasm-pack | 0.12+ | `cargo install wasm-pack` |
| Node.js | 18+ | [nodejs.org](https://nodejs.org/) |
| pnpm | 8+ | `npm install -g pnpm` |

### Docker 部署仅需要

| 工具 | 最低版本 |
|------|---------|
| Docker | 20.10+ |
| Docker Compose | 2.0+（可选） |

---

## 一、从源码构建

### 1.1 克隆代码

```bash
git clone https://github.com/hor1zon777/Portcullis.git
cd Portcullis
```

### 1.2 一键构建

**Linux / macOS / Git Bash：**

```bash
bash scripts/build-all.sh
```

**Windows PowerShell：**

```powershell
.\scripts\build-all.ps1
```

构建完成后产物：

```
target/release/captcha-server       # Linux/macOS
target/release/captcha-server.exe   # Windows
```

这个二进制文件**已包含 SDK 和 WASM**，部署时只需要这一个文件 + 配置文件。

### 1.3 手动分步构建（可选）

如果一键构建失败，可以分步排查：

```bash
# 第 1 步：构建 WASM
wasm-pack build crates/captcha-wasm --target web --out-dir ../../sdk/pkg --release

# 第 2 步：构建 SDK
cd sdk
pnpm install
pnpm build
cd ..

# 第 3 步：构建 Rust 二进制（会嵌入第 1、2 步的产物）
cargo build --release -p captcha-server
```

---

## 二、配置文件

### 2.1 生成配置模板

```bash
# Linux / macOS
./target/release/captcha-server gen-config > captcha.toml

# Windows PowerShell
.\target\release\captcha-server.exe gen-config | Out-File -Encoding utf8 captcha.toml
```

### 2.2 生成密钥

```bash
# Linux / macOS
./target/release/captcha-server gen-secret

# Windows PowerShell
.\target\release\captcha-server.exe gen-secret
```

输出一个 64 位十六进制字符串，复制它。

### 2.3 编辑配置

```toml
# captcha.toml

[server]
bind = "0.0.0.0:8787"
# 把 gen-secret 的输出粘贴到这里
secret = "粘贴你的64位十六进制密钥"
challenge_ttl_secs = 120
token_ttl_secs = 300

# 站点配置（每个业务方一个）
[[sites]]
key = "pk_mysite"                              # 公钥，放在前端 HTML 中
secret_key = "再运行一次 gen-secret 生成"         # 私钥，仅业务后端使用
diff = 18                                       # 难度，18 ≈ 1-2 秒
origins = [                                     # CORS 白名单
  "https://www.example.com",
  "https://example.com",
]

# 可选：风控配置
[risk]
dynamic_diff_enabled = true
dynamic_diff_max_increase = 4
window_size = 20
fail_rate_threshold = 0.7
blocked_ips = []
allowed_ips = []
```

### 2.4 配置说明

#### `[server]` 段

| 字段 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `bind` | 否 | `0.0.0.0:8787` | 监听地址 |
| `secret` | **是** | — | HMAC 签名主密钥，≥ 32 字节 |
| `challenge_ttl_secs` | 否 | `120` | 挑战有效期（秒） |
| `token_ttl_secs` | 否 | `300` | captcha_token 有效期（秒） |

#### `[[sites]]` 段（可多个）

| 字段 | 必填 | 说明 |
|------|------|------|
| `key` | **是** | 站点公钥，前端 `data-site-key` 使用 |
| `secret_key` | **是** | 站点私钥（≥ 16 字节），业务后端 `/siteverify` 使用 |
| `diff` | **是** | 难度（前导零比特数），推荐 14-20 |
| `origins` | 否 | CORS 白名单数组，空 = 放通所有源 |

#### `[risk]` 段

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `dynamic_diff_enabled` | `false` | 启用 IP 动态难度 |
| `dynamic_diff_max_increase` | `4` | 最大额外 diff 增量 |
| `window_size` | `20` | 滑动窗口大小（最近 N 次请求） |
| `fail_rate_threshold` | `0.7` | 触发阈值（失败率） |
| `blocked_ips` | `[]` | IP 黑名单（支持 CIDR） |
| `allowed_ips` | `[]` | IP 白名单 |

#### 难度选择参考

| diff | 桌面端 | 移动端 | 推荐场景 |
|------|--------|--------|---------|
| 14 | ~0.2s | ~0.5s | 评论、点赞 |
| 16 | ~0.5s | ~1.2s | 普通登录 |
| **18** | **~1s** | **~2.5s** | **注册、找回密码（推荐默认值）** |
| 20 | ~3s | ~8s | 高敏感操作 |

### 2.5 环境变量覆盖

所有配置都可以用环境变量覆盖（优先级：环境变量 > TOML）：

| 环境变量 | 对应配置 |
|---------|---------|
| `CAPTCHA_SECRET` | `server.secret` |
| `CAPTCHA_BIND` | `server.bind` |
| `CAPTCHA_CHALLENGE_TTL_SECS` | `server.challenge_ttl_secs` |
| `CAPTCHA_TOKEN_TTL_SECS` | `server.token_ttl_secs` |
| `CAPTCHA_SITES` | 整个 `[[sites]]` 段（JSON 格式） |
| `CAPTCHA_ADMIN_TOKEN` | `admin.token`（管理面板认证） |

```bash
# 环境变量方式启动（不需要 captcha.toml）
export CAPTCHA_SECRET="你的密钥"
export CAPTCHA_SITES='{"pk_mysite":{"secret_key":"你的站点私钥","diff":18,"origins":["https://example.com"]}}'
./captcha-server
```

---

## 三、启动服务

### 3.1 直接运行

```bash
# Linux / macOS
./target/release/captcha-server --config captcha.toml

# Windows PowerShell
.\target\release\captcha-server.exe --config captcha.toml
```

看到以下输出表示启动成功：

```
PoW 验证码服务启动：http://0.0.0.0:8787 （1 站点 | /metrics | 配置热重载已启用）
```

### 3.2 验证启动

```bash
# 健康检查
curl http://localhost:8787/healthz
# → ok

# SDK 资源
curl -I http://localhost:8787/sdk/pow-captcha.js
# → HTTP/1.1 200 OK
# → content-type: application/javascript; charset=utf-8

# Prometheus 指标
curl http://localhost:8787/metrics
```

### 3.3 后台运行（Linux）

**systemd 服务：**

```ini
# /etc/systemd/system/captcha.service
[Unit]
Description=PoW CAPTCHA Server
After=network.target

[Service]
Type=simple
ExecStart=/opt/captcha/captcha-server --config /etc/captcha/captcha.toml
Restart=always
RestartSec=5
User=captcha
Group=captcha

# 安全加固
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadOnlyPaths=/
ReadWritePaths=/var/log

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now captcha
sudo systemctl status captcha
```

### 3.4 Windows 后台运行

```powershell
# 简单方式：使用 Start-Process
Start-Process -NoNewWindow .\target\release\captcha-server.exe -ArgumentList "--config", "captcha.toml"

# 生产方式：注册为 Windows 服务（使用 NSSM）
nssm install PowCaptcha "C:\captcha\captcha-server.exe" "--config C:\captcha\captcha.toml"
nssm start PowCaptcha
```

---

## 四、前端接入

### 4.1 最简接入（推荐）

在你的 HTML 页面中加入以下代码：

```html
<!-- 第 1 步：加载 SDK（从 captcha 服务加载，自动处理 WASM） -->
<script src="https://captcha.example.com/sdk/pow-captcha.js"
        data-site-key="pk_mysite"></script>

<!-- 第 2 步：在表单中放置 widget -->
<form action="/api/login" method="POST">
  <input name="username" />
  <input name="password" type="password" />

  <!-- widget 自动渲染在这里 -->
  <div data-pow-captcha data-target="captcha_token"></div>
  <!-- 验证通过后 token 自动填入这个隐藏字段 -->
  <input type="hidden" name="captcha_token" id="captcha_token" />

  <button type="submit">登录</button>
</form>
```

**就这么多。** 不需要写任何 JavaScript 代码。

### 4.2 data 属性参考

**`<script>` 标签上：**

| 属性 | 说明 |
|------|------|
| `data-site-key` | 站点公钥（对应 `captcha.toml` 中 `[[sites]].key`） |
| `data-endpoint` | 覆盖 API 地址（默认从脚本 src 自动推导） |
| `data-wasm-base` | 覆盖 WASM 路径（默认从脚本 src 自动推导） |

**`<div data-pow-captcha>` 上：**

| 属性 | 说明 |
|------|------|
| `data-pow-captcha` | 标记为验证码容器（必需） |
| `data-target` | 验证通过后自动填充的 `<input>` 的 id |
| `data-callback` | 验证通过后调用的全局函数名 |
| `data-theme` | `light`（默认）或 `dark` |
| `data-lang` | `zh-CN`（默认）或 `en-US` |

### 4.3 编程式 API

如果需要更灵活的控制：

```html
<script src="https://captcha.example.com/sdk/pow-captcha.js"
        data-site-key="pk_mysite"></script>

<div id="my-captcha"></div>

<script>
  // SDK 加载后 window.PowCaptcha 可用
  var widget = PowCaptcha.render('#my-captcha', {
    siteKey: 'pk_mysite',
    endpoint: 'https://captcha.example.com',
    theme: 'dark',
    lang: 'en-US',
    onSuccess: function(token) {
      console.log('验证通过:', token);
    },
    onError: function(err) {
      console.error('验证失败:', err);
    },
  });

  // 重置
  widget.reset();

  // 获取当前 token
  var token = widget.getResponse();

  // 销毁
  widget.destroy();
</script>
```

---

## 五、业务后端接入

用户提交表单后，你的业务后端收到 `captcha_token` 字段，需要调 `/siteverify` 核验。

### 5.1 通用流程

```
浏览器 → 你的业务后端 → captcha 服务 /siteverify → 返回 success/fail
```

### 5.2 Node.js（Express）

```javascript
app.post('/api/login', async (req, res) => {
  const { username, password, captcha_token } = req.body;

  // 核验验证码
  const r = await fetch('https://captcha.example.com/api/v1/siteverify', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      token: captcha_token,
      secret_key: process.env.CAPTCHA_SECRET_KEY,  // 站点私钥
    }),
  });
  const data = await r.json();

  if (!data.success) {
    return res.status(403).json({ error: '验证码校验失败' });
  }

  // ... 正常业务逻辑
});
```

### 5.3 Python（FastAPI）

```python
@app.post("/api/login")
async def login(req: dict):
    async with httpx.AsyncClient(timeout=5) as c:
        r = await c.post(
            f"{CAPTCHA_ENDPOINT}/api/v1/siteverify",
            json={"token": req["captcha_token"], "secret_key": CAPTCHA_SECRET_KEY},
        )
    if not r.json().get("success"):
        raise HTTPException(403, "验证码校验失败")
    # ... 正常业务逻辑
```

### 5.4 更多语言

完整的 7 种语言代码片段：[docs/snippets/](snippets/README.md)

包括：Node.js（Express/Koa/Fastify/NestJS）、Python（FastAPI/Flask/Django）、Go（net/http/Gin）、PHP（Laravel/原生）、Java（Spring Boot）、C#（ASP.NET Core）、Ruby（Rails）。

### 5.5 接入要点

1. **必须在服务端校验** — 不要信任前端的 `onSuccess` 回调
2. **`secret_key` 只在服务端使用** — 绝不放到前端代码或 Git 仓库
3. **设置超时** — 给 siteverify 请求 3-5 秒超时
4. **失败时拒绝** — 网络错误也要拒绝请求（fail-closed）

---

## 六、Docker 部署

### 6.1 架构

Docker Compose 编排 3 个服务：

| 服务 | 说明 | 端口 |
|------|------|------|
| `captcha-server` | Rust 验证服务 | 8787（内部） |
| `admin-ui` | React 管理面板（Nginx 静态托管） | 80（内部） |
| `nginx` | 网关，统一入口 | **80（对外）** |

路由规则：
- `/admin/api/*` → captcha-server（管理 API）
- `/admin/*` → admin-ui（React SPA）
- `/*` → captcha-server（公共 API + SDK + metrics）

### 6.2 使用 docker-compose（推荐）

```bash
# 1. 准备配置文件
cp captcha.toml.example captcha.toml
# 编辑 captcha.toml（参考上面的配置说明，记得设置 [admin] 段）

# 2. 构建并启动
docker compose up -d --build

# 3. 访问
# 管理面板：http://localhost/admin/
# 公共 API：http://localhost/api/v1/challenge
# SDK：http://localhost/sdk/pow-captcha.js
# 指标：http://localhost/metrics

# 4. 查看日志
docker compose logs -f

# 5. 停止
docker compose down
```

### 6.3 健康检查

```bash
docker compose ps
# 查看各服务状态
```

---

## 七、反向代理（Nginx）

> **Docker Compose 模式**已自带 Nginx 网关（`nginx/nginx.conf`），以下配置仅用于单二进制部署 + 自行搭建 Nginx 的场景。

生产环境建议在 Nginx 后面运行，提供 HTTPS 终止。

```nginx
upstream captcha_backend {
    server 127.0.0.1:8787;
    keepalive 32;
}

server {
    listen 443 ssl http2;
    server_name captcha.example.com;

    ssl_certificate     /etc/ssl/certs/captcha.example.com.pem;
    ssl_certificate_key /etc/ssl/private/captcha.example.com.key;

    # SDK 静态资源 — 长缓存
    location /sdk/ {
        proxy_pass http://captcha_backend;
        proxy_set_header Host $host;
        proxy_cache_valid 200 1h;
        add_header X-Cache-Status $upstream_cache_status;
    }

    # API — 不缓存
    location /api/ {
        proxy_pass http://captcha_backend;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 10s;
    }

    # Prometheus — 仅内网访问
    location /metrics {
        allow 10.0.0.0/8;
        allow 172.16.0.0/12;
        deny all;
        proxy_pass http://captcha_backend;
    }

    # 健康检查
    location /healthz {
        proxy_pass http://captcha_backend;
    }
}
```

**注意**：使用反向代理时，确保 `X-Forwarded-For` 和 `X-Real-IP` 头被正确传递，否则 IP 限流和动态难度无法工作。

---

## 八、验证部署

### 8.1 端到端测试脚本

```bash
#!/bin/bash
# test-deploy.sh — 验证 captcha 服务是否正常运行
ENDPOINT="${1:-http://localhost:8787}"

echo "1. 健康检查..."
curl -sf "$ENDPOINT/healthz" && echo " ✓" || echo " ✗"

echo "2. SDK 资源..."
curl -sf -o /dev/null "$ENDPOINT/sdk/pow-captcha.js" && echo " ✓" || echo " ✗"

echo "3. WASM 资源..."
curl -sf -o /dev/null "$ENDPOINT/sdk/captcha_wasm_bg.wasm" && echo " ✓" || echo " ✗"

echo "4. Challenge 接口..."
RESP=$(curl -sf -X POST "$ENDPOINT/api/v1/challenge" \
  -H 'content-type: application/json' \
  -d '{"site_key":"pk_test"}')
echo "$RESP" | grep -q '"success":true' && echo " ✓" || echo " ✗ $RESP"

echo "5. Prometheus 指标..."
curl -sf "$ENDPOINT/metrics" | head -3 && echo " ✓" || echo " ✗"

echo "6. 压缩..."
SIZE_RAW=$(curl -sf "$ENDPOINT/sdk/pow-captcha.js" | wc -c)
SIZE_GZ=$(curl -sf -H 'Accept-Encoding: gzip' "$ENDPOINT/sdk/pow-captcha.js" | wc -c)
echo "   原始: ${SIZE_RAW}B  压缩: ${SIZE_GZ}B ✓"

echo "完成！"
```

### 8.2 浏览器测试

打开 `examples/demo.html`（或任何包含 captcha widget 的页面），按 F12 打开开发者工具：

1. **Network 面板**：确认 `pow-captcha.js` 和 `captcha_wasm_bg.wasm` 加载成功（200）
2. **Console 面板**：点击 widget，确认无报错
3. **验证流程**：点击「我不是机器人」→ 进度条走完 → 显示绿色勾 → hidden input 有值

---

## 九、监控与告警

### 9.1 Prometheus 指标

服务暴露 `/metrics` 端点，包含以下指标：

| 指标 | 类型 | 说明 |
|------|------|------|
| `captcha_challenge_issued_total` | counter | 挑战发放数 |
| `captcha_verify_success_total` | counter | 验证成功数 |
| `captcha_verify_fail_total` | counter | 验证失败数 |
| `captcha_verify_duration_seconds` | histogram | 验证延迟 |
| `captcha_siteverify_success_total` | counter | siteverify 成功数 |
| `captcha_siteverify_fail_total` | counter | siteverify 失败数 |
| `captcha_store_challenges_used` | gauge | 当前存储的 challenge 数 |
| `captcha_store_tokens_used` | gauge | 当前存储的 token 数 |

### 9.2 Grafana

导入预制面板：`docs/grafana-dashboard.json`

```bash
# Grafana UI → Dashboards → Import → Upload JSON
```

### 9.3 告警规则（Prometheus AlertManager）

```yaml
groups:
  - name: captcha
    rules:
      - alert: CaptchaHighFailRate
        expr: rate(captcha_verify_fail_total[5m]) / (rate(captcha_verify_success_total[5m]) + rate(captcha_verify_fail_total[5m])) > 0.7
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "验证失败率超过 70%"

      - alert: CaptchaServiceDown
        expr: up{job="captcha"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "CAPTCHA 服务不可达"
```

---

## 十、常见问题

### Q: CORS 报错 "No 'Access-Control-Allow-Origin' header"

**原因**：前端页面的源（origin）不在 `captcha.toml` 的 `origins` 白名单中。

**解决**：在 `[[sites]].origins` 中加入你的前端地址：

```toml
origins = ["https://www.example.com", "http://localhost:3000"]
```

修改后 30 秒内自动热重载，无需重启。

### Q: widget 显示「浏览器不兼容」

**原因**：浏览器不支持 WebAssembly（极旧浏览器或特殊 WebView）。

**解决**：升级浏览器。WebAssembly 在 Chrome 57+、Firefox 52+、Safari 11+、Edge 16+ 均支持。

### Q: 验证耗时太长

**解决**：降低 `diff` 值。每减少 2，耗时约降为 1/4。

```toml
diff = 16  # 从 18 降到 16，耗时 ~1s → ~0.5s
```

### Q: 多台服务器部署时防重放失效

**原因**：v1.0.0 使用内存存储，每台服务器各自维护重放记录。

**解决**：
- 使用负载均衡的 session sticky（按 IP 路由到固定节点）
- 或等待 v1.1 的 Redis 存储后端

### Q: 配置修改后不生效

服务每 30 秒检查 `captcha.toml` 变更，如需立即生效请重启服务。

**注意**：环境变量优先级高于 TOML。如果同时设了环境变量和 TOML，环境变量生效。

### Q: `secret` 和 `secret_key` 的区别

| | `secret` | `secret_key` |
|---|---------|-------------|
| 位置 | `[server]` 段 | `[[sites]]` 段 |
| 用途 | 签名挑战和 token（内部） | 业务后端调 `/siteverify` 的凭证 |
| 谁持有 | 仅 captcha 服务 | captcha 服务 + 业务后端 |
| 长度要求 | ≥ 32 字节 | ≥ 16 字节 |

### Q: Windows 上 `./captcha-server` 无法识别

Windows PowerShell 需要用 `.\` 而非 `./`，且需要完整路径：

```powershell
.\target\release\captcha-server.exe --config captcha.toml
```

### Q: 如何升级 Argon2 参数

参见 [UPGRADING.md](UPGRADING.md)。**必须同时重新构建 WASM 和服务端**，灰度发布。
