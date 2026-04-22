# Node.js 业务后端接入

## 通用函数

```javascript
async function verifyCaptcha(token, endpoint, secretKey) {
  const res = await fetch(`${endpoint}/api/v1/siteverify`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ token, secret_key: secretKey }),
  });
  const data = await res.json();
  return data.success === true;
}
```

## Express

```javascript
import express from 'express';

const app = express();
app.use(express.json());

const CAPTCHA_ENDPOINT = process.env.CAPTCHA_ENDPOINT;
const CAPTCHA_SECRET_KEY = process.env.CAPTCHA_SECRET_KEY;

app.post('/api/login', async (req, res) => {
  const { username, password, captcha_token } = req.body;

  if (!captcha_token) return res.status(400).json({ error: '缺少验证码' });

  const ok = await verifyCaptcha(captcha_token, CAPTCHA_ENDPOINT, CAPTCHA_SECRET_KEY);
  if (!ok) return res.status(403).json({ error: '验证码校验失败' });

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
const router = new Router();

router.post('/api/login', async (ctx) => {
  const { captcha_token } = ctx.request.body;
  const ok = await verifyCaptcha(captcha_token, process.env.CAPTCHA_ENDPOINT, process.env.CAPTCHA_SECRET_KEY);
  if (!ok) {
    ctx.status = 403;
    ctx.body = { error: '验证码校验失败' };
    return;
  }
  ctx.body = { success: true };
});

app.use(bodyParser()).use(router.routes());
```

## Fastify

```javascript
import Fastify from 'fastify';

const fastify = Fastify();

fastify.post('/api/login', async (req, reply) => {
  const { captcha_token } = req.body;
  const ok = await verifyCaptcha(captcha_token, process.env.CAPTCHA_ENDPOINT, process.env.CAPTCHA_SECRET_KEY);
  if (!ok) return reply.code(403).send({ error: '验证码校验失败' });
  return { success: true };
});
```

## NestJS（Guard 模式）

```typescript
import { CanActivate, ExecutionContext, Injectable, ForbiddenException } from '@nestjs/common';

@Injectable()
export class CaptchaGuard implements CanActivate {
  async canActivate(ctx: ExecutionContext): Promise<boolean> {
    const req = ctx.switchToHttp().getRequest();
    const token = req.body?.captcha_token;
    if (!token) throw new ForbiddenException('缺少验证码');

    const res = await fetch(`${process.env.CAPTCHA_ENDPOINT}/api/v1/siteverify`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ token, secret_key: process.env.CAPTCHA_SECRET_KEY }),
    });
    const data = await res.json();
    if (!data.success) throw new ForbiddenException('验证码校验失败');
    return true;
  }
}

// 用法：@UseGuards(CaptchaGuard) on controller method
```
