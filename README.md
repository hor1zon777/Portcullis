# PoW CAPTCHA

基于工作量证明（Proof of Work）的验证码服务。Argon2id 内存硬化 + SHA-256 快速迭代，单二进制部署，一行 `<script>` 接入。

## 一分钟接入

### 1. 部署服务

```bash
# Docker（推荐）
docker run -d -p 8787:8787 \
  -v $(pwd)/captcha.toml:/etc/captcha/captcha.toml:ro \
  pow-captcha:latest

# 或直接运行二进制
./captcha-server gen-config > captcha.toml   # 生成配置模板
./captcha-server gen-secret                   # 生成密钥
# 编辑 captcha.toml 填入密钥和站点配置
./captcha-server --config captcha.toml
```

### 2. 前端接入（零 JS 代码）

```html
<script src="https://captcha.example.com/sdk/pow-captcha.js"
        data-site-key="pk_test"></script>

<form>
  <div data-pow-captcha data-target="captcha_token"></div>
  <input type="hidden" name="captcha_token" id="captcha_token" />
  <button type="submit">提交</button>
</form>
```

widget 自动渲染，验证通过后自动填充 `#captcha_token`。

### 3. 后端校验

```bash
curl -X POST https://captcha.example.com/api/v1/siteverify \
  -H 'content-type: application/json' \
  -d '{"token":"<captcha_token>","secret_key":"sk_test_secret"}'
# → {"success": true, "challenge_id": "…", "site_key": "pk_test"}
```

7 种语言的后端接入代码片段：[docs/snippets/](docs/snippets/README.md)

---

## 工作原理

```
浏览器                                    验证服务
  │  ① 请求挑战 ──────────────────────►  │  发放 challenge + HMAC 签名
  │  ② 本地挖矿：                        │
  │     Argon2id(challenge) → base_hash  │  （一次性，~100ms）
  │     SHA-256(base ‖ nonce) 循环迭代    │  （~1-2 秒找到解）
  │  ③ 提交 nonce ────────────────────►  │  单次 Argon2+SHA-256 验证，发放 token
  │  ④ token 随表单提交到业务后端 ──►     │
  │                       业务后端 ──────►│  ⑤ /siteverify 核验 token
```

- **Argon2id（4 MiB）** 保证每个 challenge 的内存硬化成本，对抗 GPU 农场
- **SHA-256 快速迭代** 保证 1-2 秒内完成，UI 不阻塞（chunked 主线程执行）
- **HMAC-SHA256 签名** 保证 challenge 不可伪造，服务端无状态发放

## 部署方式

### 方式 A：单二进制

服务端在编译期嵌入 SDK + WASM，部署只需一个可执行文件。

```bash
# 从源码构建（需要 Rust + Node + wasm-pack）
bash scripts/build-all.sh

# 运行
./target/release/captcha-server --config captcha.toml
```

### 方式 B：Docker

```bash
docker compose up -d   # 使用 captcha.toml 配置
```

### 方式 C：开发模式

```bash
# 终端 A：验证服务
export CAPTCHA_SECRET="dev-secret-must-be-at-least-32-bytes!!"
export CAPTCHA_SITES='{"pk_test":{"secret_key":"sk_test_secret","diff":18,"origins":["http://localhost:5173"]}}'
cargo run -p captcha-server

# 终端 B：SDK 开发服务器
cd sdk && pnpm install && pnpm dev
# 打开 http://localhost:5173
```

## 配置

`captcha.toml`（或环境变量，env 优先级更高）：

```toml
[server]
bind = "0.0.0.0:8787"
secret = "运行 captcha-server gen-secret 生成"
challenge_ttl_secs = 120
token_ttl_secs = 300

[[sites]]
key = "pk_test"
secret_key = "sk_test_secret"
diff = 18
origins = ["https://example.com"]
```

## 难度参考

| diff | 期望尝试 | 桌面端 | 移动端 | 适用场景 |
|------|----------|--------|--------|---------|
| 14   | 16 K     | ~0.2s  | ~0.5s  | 评论 / 点赞 |
| 16   | 65 K     | ~0.5s  | ~1.2s  | 普通登录 |
| 18   | 262 K    | ~1s    | ~2.5s  | 默认（注册、找回密码） |
| 20   | 1 M      | ~3s    | ~8s    | 高敏感操作 |

## 项目结构

```
captcha/
├── crates/
│   ├── captcha-core/     # 共享算法库（Argon2id + SHA-256 + HMAC）
│   ├── captcha-server/   # HTTP 服务（嵌入 SDK 和 WASM 静态资源）
│   └── captcha-wasm/     # 浏览器端求解器（WebAssembly）
├── sdk/                  # 浏览器 SDK（TypeScript、构建为 IIFE）
├── docs/
│   ├── PROTOCOL.md       # 协议规范
│   ├── INTEGRATION.md    # 接入指南
│   ├── SECURITY.md       # 威胁模型 + 加固清单
│   └── snippets/         # 7 种语言的后端接入代码
├── Dockerfile            # 4 阶段构建
├── docker-compose.yml
├── captcha.toml.example  # 配置模板
└── scripts/              # 构建与开发脚本
```

## 文档

| 文档 | 说明 |
|------|------|
| [协议规范](docs/PROTOCOL.md) | 挑战/解答/签名格式，双阶段算法细节 |
| [接入指南](docs/INTEGRATION.md) | 部署、配置、前后端接入 |
| [安全加固](docs/SECURITY.md) | 威胁模型、12 项安全对策、加固清单 |
| [后端代码片段](docs/snippets/README.md) | Node/Python/Go/PHP/Java/C#/Ruby |

## 运行测试

```bash
cargo test                     # 所有 Rust 测试（35 个）
cd sdk && pnpm type-check      # SDK 类型检查
```

## License

MIT
