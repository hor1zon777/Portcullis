# 业务后端接入 · 代码片段索引

业务后端通过 `POST /api/v1/siteverify` 核验浏览器提交的 `captcha_token`。

## 请求体

| 字段 | 自 | 必填 | 说明 |
|------|----|------|------|
| `token` | v1.0 | 是 | 浏览器表单提交的 captcha_token |
| `secret_key` | v1.0 | 是 | 站点明文 secret_key（业务方持有；v1.5+ 服务端内部做 HMAC 再比对） |
| `client_ip` | v1.4 | 条件 | 站点启用 `bind_token_to_ip` 时必填；合法 IPv4/IPv6 字符串 |
| `user_agent` | v1.4 | 条件 | 站点启用 `bind_token_to_ua` 时必填；原串（与 /verify 时一致） |

```json
{
  "token": "<captcha_token>",
  "secret_key": "<sk_xxx>",
  "client_ip": "203.0.113.5",
  "user_agent": "Mozilla/5.0 ..."
}
```

## 响应体

成功：
```json
{ "success": true, "challenge_id": "...", "site_key": "..." }
```

失败（始终 HTTP 200 OK）：
```json
{ "success": false, "error": "token 无效或已过期" }
```

---

## 按语言查找

| 语言 | 文件 | 覆盖框架 |
|------|------|---------|
| Node.js / TypeScript | [nodejs.md](./nodejs.md) | Express, Koa, Fastify, NestJS |
| Python | [python.md](./python.md) | FastAPI, Flask, Django |
| Go | [go.md](./go.md) | net/http, Gin |
| PHP | [php.md](./php.md) | 原生, Laravel, Slim |
| Java | [java.md](./java.md) | JDK HttpClient, Spring Boot |
| C# / .NET | [csharp.md](./csharp.md) | ASP.NET Core |
| Ruby | [ruby.md](./ruby.md) | Rails, Sinatra |

> 推荐首先精读 [nodejs.md](./nodejs.md) 或 [python.md](./python.md)，它们涵盖 v1.4+ 身份绑定字段的完整调用方式；其他语言同样支持这些字段（直接在 JSON 体里加 `client_ip` / `user_agent` 即可）。

---

## 最小可用 curl 示例

v1.0 基础：
```bash
curl -X POST https://captcha.example.com/api/v1/siteverify \
  -H "content-type: application/json" \
  -d '{"token":"...", "secret_key":"sk_..."}'
```

v1.4+ 启用绑定时：
```bash
curl -X POST https://captcha.example.com/api/v1/siteverify \
  -H "content-type: application/json" \
  -d '{
    "token":"...",
    "secret_key":"sk_...",
    "client_ip":"203.0.113.5",
    "user_agent":"Mozilla/5.0 ..."
  }'
```

---

## 接入要点

1. **始终在服务端校验**：不要信任前端 `onSuccess` 回调，前端 token 必须经 siteverify 核验
2. **密钥隔离**：`secret_key` 只存在于业务后端环境变量 / secrets manager，**绝不放到前端代码**
3. **secret_key 备份**：v1.5+ 服务端只保留 HMAC，业务方持有的明文**必须保存到 Vault / 密码管理器**（创建站点时管理面板会一次性返回并自动复制到剪贴板）
4. **超时控制**：给 siteverify 请求 3~5 秒超时，避免 captcha 服务故障拖累业务
5. **短期缓存（可选）**：同一 `token` 第一次 success 后可以在业务后端缓存 30 秒，防止重复核验产生的延迟
6. **错误处理**：siteverify 网络错误时应**拒绝**请求（fail-closed），不要因为网络抖动放行
7. **启用身份绑定时**（v1.4+）：
   - 确认反向代理正确透传 `X-Forwarded-For` / `X-Real-IP`（否则 `/verify` 会直接 400）
   - 业务后端传入的 `client_ip` 必须和浏览器真实 IP 一致；在 Express 里需 `app.set('trust proxy', true)` 才能拿到正确 `req.ip`
   - `user_agent` 要原样透传（部分中间件可能改写），推荐 `req.headers['user-agent']`
