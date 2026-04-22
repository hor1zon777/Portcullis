# 业务后端接入 · 代码片段索引

业务后端通过 `POST /api/v1/siteverify` 核验浏览器提交的 `captcha_token`。
接口固定：

**请求体**
```json
{ "token": "<captcha_token>", "secret_key": "<sk_xxx>" }
```

**响应体**
```json
{ "success": true, "challenge_id": "...", "site_key": "..." }
```

`secret_key` 对应 `captcha.toml` 中该站点的 `[[sites]].secret_key`，不要泄露到前端。

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

---

## 最小可用 curl 示例

```bash
curl -X POST https://captcha.example.com/api/v1/siteverify \
  -H "content-type: application/json" \
  -d '{"token":"...", "secret_key":"sk_..."}'
```

---

## 接入要点

1. **始终在服务端校验**：不要信任前端 `onSuccess` 回调，前端 token 必须经 siteverify 核验
2. **密钥隔离**：`secret_key` 只存在于业务后端环境变量 / secrets manager，**绝不放到前端代码**
3. **超时控制**：给 siteverify 请求 3~5 秒超时，避免 captcha 服务故障拖累业务
4. **短期缓存（可选）**：同一 `token` 第一次 success 后可以在业务后端缓存 30 秒，防止重复核验产生的延迟
5. **错误处理**：siteverify 网络错误时应**拒绝**请求（fail-closed），不要因为网络抖动放行
