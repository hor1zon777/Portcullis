# Node.js 业务后端接入

> v1.4+ 新增 `client_ip` / `user_agent` 字段，用于 opt-in 身份绑定。下面的示例都已携带，**未启用绑定的站点会忽略这两个字段**，保持向后兼容。

## 通用函数（v1.4+）

```javascript
async function verifyCaptcha({ token, endpoint, secretKey, clientIp, userAgent }) {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), 3000);
  try {
    const res = await fetch(`${endpoint}/api/v1/siteverify`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      signal: ctrl.signal,
      body: JSON.stringify({
        token,
        secret_key: secretKey,
        client_ip: clientIp,      // v1.4+，启用 bind_token_to_ip 时必填
        user_agent: userAgent,    // v1.4+，启用 bind_token_to_ua 时必填
      }),
    });
    const data = await res.json();
    return { ok: data.success === true, error: data.error };
  } catch (e) {
    // 网络异常一律 fail-closed
    return { ok: false, error: String(e) };
  } finally {
    clearTimeout(timer);
  }
}
```

## Express

```javascript
import express from 'express';

const app = express();
app.use(express.json());
app.set('trust proxy', true);  // 启用 bind_token_to_ip 时必须，否则 req.ip 拿不到真实 IP

const CAPTCHA_ENDPOINT = process.env.CAPTCHA_ENDPOINT;
const CAPTCHA_SECRET_KEY = process.env.CAPTCHA_SECRET_KEY;  // 明文 secret_key，创建站点时一次性保存

app.post('/api/login', async (req, res) => {
  const { username, password, captcha_token } = req.body;

  if (!captcha_token) return res.status(400).json({ error: '缺少验证码' });

  const { ok, error } = await verifyCaptcha({
    token: captcha_token,
    endpoint: CAPTCHA_ENDPOINT,
    secretKey: CAPTCHA_SECRET_KEY,
    clientIp: req.ip,
    userAgent: req.headers['user-agent'],
  });
  if (!ok) return res.status(403).json({ error: error || '验证码校验失败' });

  // ... 业务逻辑
  res.json({ success: true });
});
```

## Koa

```javascript
import Koa from 'koa';
import Router from '@koa/router';
import bodyParser from 'koa-bodyparser';

const app = new Koa();
app.proxy = true;  // 启用 bind_token_to_ip 时必须
const router = new Router();

router.post('/api/login', async (ctx) => {
  const { captcha_token } = ctx.request.body;
  const { ok, error } = await verifyCaptcha({
    token: captcha_token,
    endpoint: process.env.CAPTCHA_ENDPOINT,
    secretKey: process.env.CAPTCHA_SECRET_KEY,
    clientIp: ctx.ip,
    userAgent: ctx.request.headers['user-agent'],
  });
  if (!ok) {
    ctx.status = 403;
    ctx.body = { error: error || '验证码校验失败' };
    return;
  }
  ctx.body = { success: true };
});

app.use(bodyParser()).use(router.routes());
```

## Fastify

```javascript
import Fastify from 'fastify';

const fastify = Fastify({
  trustProxy: true,  // 启用 bind_token_to_ip 时必须
});

fastify.post('/api/login', async (req, reply) => {
  const { captcha_token } = req.body;
  const { ok, error } = await verifyCaptcha({
    token: captcha_token,
    endpoint: process.env.CAPTCHA_ENDPOINT,
    secretKey: process.env.CAPTCHA_SECRET_KEY,
    clientIp: req.ip,
    userAgent: req.headers['user-agent'],
  });
  if (!ok) return reply.code(403).send({ error: error || '验证码校验失败' });
  return { success: true };
});
```

## NestJS（Guard 模式）

```typescript
import { CanActivate, ExecutionContext, Injectable, ForbiddenException } from '@nestjs/common';
import { Request } from 'express';

@Injectable()
export class CaptchaGuard implements CanActivate {
  async canActivate(ctx: ExecutionContext): Promise<boolean> {
    const req = ctx.switchToHttp().getRequest<Request>();
    const token = req.body?.captcha_token;
    if (!token) throw new ForbiddenException('缺少验证码');

    // 注意：NestJS 底层的 Express 也需要 app.set('trust proxy', true)
    const payload = {
      token,
      secret_key: process.env.CAPTCHA_SECRET_KEY,
      client_ip: req.ip,
      user_agent: req.headers['user-agent'],
    };
    const res = await fetch(`${process.env.CAPTCHA_ENDPOINT}/api/v1/siteverify`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(payload),
    });
    const data = await res.json();
    if (!data.success) throw new ForbiddenException(data.error || '验证码校验失败');
    return true;
  }
}

// 用法：@UseGuards(CaptchaGuard) on controller method
```

## 生产注意事项

- **反向代理**：如果 Nginx/Caddy 做了 TLS 终止，**必须**透传 `X-Forwarded-For` / `X-Real-IP`，否则 req.ip 拿到的是代理 IP，会被 siteverify 的 IP 绑定判定为"不匹配"。详见 [`docs/DEPLOY.md`](../DEPLOY.md#71-v140启用-bind_token_to_ip-时的强制要求) §7.1。
- **`secret_key` 获取**：v1.5+ 在管理面板「站点」页新建站点时，创建响应里**一次性**返回明文并自动复制到剪贴板。**必须**立即存入 Vault / 密码管理器 / `.env`；服务端此后只保留 HMAC，无法再次取出原文。
- **CAPTCHA_SECRET 轮换**：如果 captcha 服务端更换主密钥（v1.5+ 的双 key 轮换流程），需要同步重建所有站点的 secret_key 并更新业务后端 env。
