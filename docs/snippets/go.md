# Go 业务后端接入

> v1.4+ 新增 `client_ip` / `user_agent` 字段用于 opt-in 身份绑定。下面的示例都已携带；**未启用绑定的站点会忽略这两个字段**，保持向后兼容。

## 通用函数

```go
package captcha

import (
    "bytes"
    "context"
    "encoding/json"
    "net/http"
    "time"
)

type SiteVerifyResp struct {
    Success     bool   `json:"success"`
    ChallengeID string `json:"challenge_id,omitempty"`
    SiteKey     string `json:"site_key,omitempty"`
    Error       string `json:"error,omitempty"`
}

// Verify 核验 captcha_token。v1.4+ 可选传入 clientIP / userAgent。
func Verify(ctx context.Context, token, endpoint, secretKey, clientIP, userAgent string) (*SiteVerifyResp, error) {
    payload := map[string]string{
        "token":      token,
        "secret_key": secretKey,
    }
    if clientIP != "" {
        payload["client_ip"] = clientIP
    }
    if userAgent != "" {
        payload["user_agent"] = userAgent
    }
    body, _ := json.Marshal(payload)

    req, _ := http.NewRequestWithContext(ctx, http.MethodPost,
        endpoint+"/api/v1/siteverify", bytes.NewReader(body))
    req.Header.Set("Content-Type", "application/json")

    client := &http.Client{Timeout: 5 * time.Second}
    resp, err := client.Do(req)
    if err != nil {
        return nil, err
    }
    defer resp.Body.Close()

    var r SiteVerifyResp
    if err := json.NewDecoder(resp.Body).Decode(&r); err != nil {
        return nil, err
    }
    return &r, nil
}

// 从请求头提取真实客户端 IP（反代后必须这样做，否则拿到的是代理 IP）
func ClientIP(r *http.Request) string {
    if xff := r.Header.Get("X-Forwarded-For"); xff != "" {
        if i := bytes.IndexByte([]byte(xff), ','); i > 0 {
            return string(bytes.TrimSpace([]byte(xff[:i])))
        }
        return xff
    }
    if xri := r.Header.Get("X-Real-IP"); xri != "" {
        return xri
    }
    return r.RemoteAddr
}
```

## 标准库 net/http

```go
package main

import (
    "context"
    "encoding/json"
    "net/http"
    "os"
)

func loginHandler(w http.ResponseWriter, r *http.Request) {
    var body struct {
        CaptchaToken string `json:"captcha_token"`
    }
    json.NewDecoder(r.Body).Decode(&body)

    resp, err := captcha.Verify(r.Context(),
        body.CaptchaToken,
        os.Getenv("CAPTCHA_ENDPOINT"),
        os.Getenv("CAPTCHA_SECRET_KEY"),
        captcha.ClientIP(r),                       // v1.4+
        r.Header.Get("User-Agent"),                 // v1.4+
    )
    if err != nil || !resp.Success {
        msg := "验证码校验失败"
        if resp != nil && resp.Error != "" {
            msg = resp.Error
        }
        http.Error(w, msg, http.StatusForbidden)
        return
    }
    w.Write([]byte(`{"success":true}`))
}

func main() {
    http.HandleFunc("/api/login", loginHandler)
    http.ListenAndServe(":8080", nil)
}
```

## Gin 中间件

```go
import "github.com/gin-gonic/gin"

func CaptchaMiddleware(endpoint, secretKey string) gin.HandlerFunc {
    return func(c *gin.Context) {
        var body struct {
            CaptchaToken string `json:"captcha_token"`
        }
        if err := c.ShouldBindJSON(&body); err != nil {
            c.AbortWithStatusJSON(400, gin.H{"error": "invalid body"})
            return
        }
        resp, err := captcha.Verify(c.Request.Context(),
            body.CaptchaToken, endpoint, secretKey,
            c.ClientIP(),                       // Gin 会自动解析 X-Forwarded-For
            c.Request.Header.Get("User-Agent"),
        )
        if err != nil || !resp.Success {
            msg := "验证码校验失败"
            if resp != nil && resp.Error != "" {
                msg = resp.Error
            }
            c.AbortWithStatusJSON(403, gin.H{"error": msg})
            return
        }
        c.Set("captcha_verified", true)
        c.Next()
    }
}
```

## 生产注意事项

- `c.ClientIP()`（Gin）默认使用 `X-Forwarded-For`，但需要通过 `engine.SetTrustedProxies([]string{"127.0.0.1"})` 显式信任反代
- 启用 `bind_token_to_ip` 时务必确认反代正确透传 IP（详见 [`docs/DEPLOY.md`](../DEPLOY.md) §7.1）
- v1.5+ 的 `secret_key` 是一次性明文，创建站点时必须保存到密钥管理器

