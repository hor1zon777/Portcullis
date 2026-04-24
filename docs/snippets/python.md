# Python 业务后端接入

> v1.4+ 新增 `client_ip` / `user_agent` 字段，用于 opt-in 身份绑定。下面的示例都已携带，**未启用绑定的站点会忽略这两个字段**，保持向后兼容。

## 通用函数（同步）

```python
import requests

def verify_captcha(
    token: str,
    endpoint: str,
    secret_key: str,
    client_ip: str | None = None,     # v1.4+
    user_agent: str | None = None,    # v1.4+
) -> tuple[bool, str | None]:
    payload = {"token": token, "secret_key": secret_key}
    if client_ip is not None:
        payload["client_ip"] = client_ip
    if user_agent is not None:
        payload["user_agent"] = user_agent
    try:
        r = requests.post(
            f"{endpoint}/api/v1/siteverify",
            json=payload,
            timeout=5,
        )
        data = r.json()
        return data.get("success") is True, data.get("error")
    except Exception as e:
        # 网络异常 fail-closed
        return False, str(e)
```

## FastAPI（async）

```python
import os
import httpx
from fastapi import FastAPI, HTTPException, Body, Request

app = FastAPI()
CAPTCHA_ENDPOINT = os.environ["CAPTCHA_ENDPOINT"]
CAPTCHA_SECRET_KEY = os.environ["CAPTCHA_SECRET_KEY"]  # 创建站点时一次性保存的明文

async def verify_captcha(
    token: str,
    client_ip: str | None = None,
    user_agent: str | None = None,
) -> tuple[bool, str | None]:
    payload = {"token": token, "secret_key": CAPTCHA_SECRET_KEY}
    if client_ip is not None:
        payload["client_ip"] = client_ip
    if user_agent is not None:
        payload["user_agent"] = user_agent
    try:
        async with httpx.AsyncClient(timeout=5) as c:
            r = await c.post(f"{CAPTCHA_ENDPOINT}/api/v1/siteverify", json=payload)
        data = r.json()
        return data.get("success") is True, data.get("error")
    except Exception as e:
        return False, str(e)


def extract_client_ip(request: Request) -> str:
    # 启用 bind_token_to_ip 时必须：从反代透传的 XFF 头取真实 IP
    xff = request.headers.get("x-forwarded-for")
    if xff:
        return xff.split(",")[0].strip()
    xri = request.headers.get("x-real-ip")
    if xri:
        return xri.strip()
    return request.client.host if request.client else ""


@app.post("/api/login")
async def login(request: Request, payload: dict = Body(...)):
    ok, err = await verify_captcha(
        token=payload.get("captcha_token", ""),
        client_ip=extract_client_ip(request),
        user_agent=request.headers.get("user-agent", ""),
    )
    if not ok:
        raise HTTPException(403, err or "验证码校验失败")
    # ... 业务逻辑
    return {"success": True}
```

## Flask

```python
import os
from flask import Flask, request, jsonify

app = Flask(__name__)
# 若 Flask 运行在反代后，启用 ProxyFix 以便 request.remote_addr 是真实 IP
from werkzeug.middleware.proxy_fix import ProxyFix
app.wsgi_app = ProxyFix(app.wsgi_app, x_for=1, x_proto=1)


@app.post("/api/login")
def login():
    token = request.json.get("captcha_token")
    ok, err = verify_captcha(
        token=token,
        endpoint=os.environ["CAPTCHA_ENDPOINT"],
        secret_key=os.environ["CAPTCHA_SECRET_KEY"],
        client_ip=request.remote_addr,
        user_agent=request.headers.get("User-Agent", ""),
    )
    if not ok:
        return jsonify(error=err or "验证码校验失败"), 403
    return jsonify(success=True)
```

## Django（中间件/装饰器）

```python
from functools import wraps
from django.http import JsonResponse
from django.conf import settings

# settings.py 中启用 SECURE_PROXY_SSL_HEADER / USE_X_FORWARDED_HOST
# 让 request.META["REMOTE_ADDR"] 反映真实 IP（或显式解析 HTTP_X_FORWARDED_FOR）


def _client_ip(request) -> str:
    xff = request.META.get("HTTP_X_FORWARDED_FOR", "")
    if xff:
        return xff.split(",")[0].strip()
    return request.META.get("REMOTE_ADDR", "")


def require_captcha(view_func):
    @wraps(view_func)
    def wrapper(request, *args, **kwargs):
        import json
        body = json.loads(request.body)
        token = body.get("captcha_token")
        ok, err = verify_captcha(
            token=token,
            endpoint=settings.CAPTCHA_ENDPOINT,
            secret_key=settings.CAPTCHA_SECRET_KEY,
            client_ip=_client_ip(request),
            user_agent=request.META.get("HTTP_USER_AGENT", ""),
        )
        if not ok:
            return JsonResponse({"error": err or "验证码校验失败"}, status=403)
        return view_func(request, *args, **kwargs)
    return wrapper

# 用法：@require_captcha 装饰 view 函数
```

## 生产注意事项

- **反向代理透传 IP**：启用 `bind_token_to_ip` 后，业务后端必须从 `X-Forwarded-For` / `X-Real-IP` 取真实客户端 IP（详见 [`docs/DEPLOY.md`](../DEPLOY.md) §7.1）。
- **`secret_key` 一次性保存**：v1.5+ captcha 服务端只保留 HMAC，业务后端持有的明文必须存入 Vault / 密码管理器 / `.env`。管理面板创建站点时会一次性返回明文并自动复制到剪贴板，错过即无法找回。
- **UA 稳定性**：启用 `bind_token_to_ua` 时建议搭配短 `token_ttl_secs`（60~120 秒），避免浏览器自动升级带来的漂移。
