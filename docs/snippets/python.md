# Python 业务后端接入

## 通用函数（同步）

```python
import requests

def verify_captcha(token: str, endpoint: str, secret_key: str) -> bool:
    r = requests.post(
        f"{endpoint}/api/v1/siteverify",
        json={"token": token, "secret_key": secret_key},
        timeout=5,
    )
    return r.json().get("success") is True
```

## FastAPI（async）

```python
import os
import httpx
from fastapi import FastAPI, HTTPException, Body

app = FastAPI()
CAPTCHA_ENDPOINT = os.environ["CAPTCHA_ENDPOINT"]
CAPTCHA_SECRET_KEY = os.environ["CAPTCHA_SECRET_KEY"]

async def verify_captcha(token: str) -> bool:
    async with httpx.AsyncClient(timeout=5) as c:
        r = await c.post(
            f"{CAPTCHA_ENDPOINT}/api/v1/siteverify",
            json={"token": token, "secret_key": CAPTCHA_SECRET_KEY},
        )
    return r.json().get("success") is True

@app.post("/api/login")
async def login(payload: dict = Body(...)):
    if not await verify_captcha(payload.get("captcha_token", "")):
        raise HTTPException(403, "验证码校验失败")
    # ... 业务逻辑
    return {"success": True}
```

## Flask

```python
from flask import Flask, request, jsonify

app = Flask(__name__)

@app.post("/api/login")
def login():
    token = request.json.get("captcha_token")
    if not verify_captcha(token, os.environ["CAPTCHA_ENDPOINT"], os.environ["CAPTCHA_SECRET_KEY"]):
        return jsonify(error="验证码校验失败"), 403
    return jsonify(success=True)
```

## Django（中间件/装饰器）

```python
from functools import wraps
from django.http import JsonResponse

def require_captcha(view_func):
    @wraps(view_func)
    def wrapper(request, *args, **kwargs):
        import json
        body = json.loads(request.body)
        token = body.get("captcha_token")
        if not verify_captcha(token, settings.CAPTCHA_ENDPOINT, settings.CAPTCHA_SECRET_KEY):
            return JsonResponse({"error": "验证码校验失败"}, status=403)
        return view_func(request, *args, **kwargs)
    return wrapper

# 用法：@require_captcha 装饰 view 函数
```
