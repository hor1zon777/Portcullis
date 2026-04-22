/**
 * Node.js 业务后端示例：演示 /siteverify 接入方式。
 *
 * 启动：
 *   cd examples/backend-node
 *   npm install
 *   CAPTCHA_ENDPOINT=http://localhost:8787 CAPTCHA_SECRET_KEY=sk_test_secret node server.js
 */

import express from 'express';

const app = express();
app.use(express.json());

const CAPTCHA_ENDPOINT =
  process.env.CAPTCHA_ENDPOINT || 'http://localhost:8787';
const CAPTCHA_SECRET_KEY =
  process.env.CAPTCHA_SECRET_KEY || 'sk_test_secret';

/**
 * 核验 captcha_token。
 * @param {string} token - 浏览器提交的 captcha_token
 * @returns {Promise<{success: boolean, error?: string}>}
 */
async function verifyCaptcha(token) {
  const res = await fetch(`${CAPTCHA_ENDPOINT}/api/v1/siteverify`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      token,
      secret_key: CAPTCHA_SECRET_KEY,
    }),
  });
  return res.json();
}

// 登录端点
app.post('/api/login', async (req, res) => {
  const { username, password, captcha_token } = req.body;

  if (!captcha_token) {
    return res.status(400).json({ error: '缺少 captcha_token' });
  }

  // 1. 验证码校验
  const captchaResult = await verifyCaptcha(captcha_token);
  if (!captchaResult.success) {
    return res.status(403).json({
      error: '验证码校验失败',
      detail: captchaResult.error,
    });
  }

  // 2. 业务逻辑（此处仅演示）
  console.log(`[login] 用户 ${username} 登录，验证码通过`);
  console.log(`[login] challenge_id=${captchaResult.challenge_id}`);

  res.json({
    success: true,
    message: `欢迎 ${username}`,
    session: 'mock-session-token',
  });
});

const PORT = process.env.PORT || 3000;
app.listen(PORT, () => {
  console.log(`业务后端示例运行中: http://localhost:${PORT}`);
  console.log(`验证服务地址: ${CAPTCHA_ENDPOINT}`);
});
