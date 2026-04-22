# Go 业务后端接入

## 通用函数

```go
package captcha

import (
    "bytes"
    "encoding/json"
    "net/http"
    "time"
)

type siteVerifyResp struct {
    Success bool `json:"success"`
}

func Verify(token, endpoint, secretKey string) (bool, error) {
    body, _ := json.Marshal(map[string]string{
        "token":      token,
        "secret_key": secretKey,
    })
    client := &http.Client{Timeout: 5 * time.Second}
    resp, err := client.Post(endpoint+"/api/v1/siteverify", "application/json", bytes.NewReader(body))
    if err != nil {
        return false, err
    }
    defer resp.Body.Close()

    var r siteVerifyResp
    if err := json.NewDecoder(resp.Body).Decode(&r); err != nil {
        return false, err
    }
    return r.Success, nil
}
```

## 标准库 net/http

```go
package main

import (
    "encoding/json"
    "net/http"
    "os"
)

func loginHandler(w http.ResponseWriter, r *http.Request) {
    var body struct {
        CaptchaToken string `json:"captcha_token"`
    }
    json.NewDecoder(r.Body).Decode(&body)

    ok, err := captcha.Verify(body.CaptchaToken,
        os.Getenv("CAPTCHA_ENDPOINT"),
        os.Getenv("CAPTCHA_SECRET_KEY"))
    if err != nil || !ok {
        http.Error(w, "验证码校验失败", http.StatusForbidden)
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

func CaptchaMiddleware() gin.HandlerFunc {
    return func(c *gin.Context) {
        var body struct {
            CaptchaToken string `json:"captcha_token"`
        }
        if err := c.ShouldBindJSON(&body); err != nil {
            c.AbortWithStatusJSON(400, gin.H{"error": "invalid body"})
            return
        }
        ok, _ := captcha.Verify(body.CaptchaToken, endpoint, secretKey)
        if !ok {
            c.AbortWithStatusJSON(403, gin.H{"error": "验证码校验失败"})
            return
        }
        // 传递给后续 handler
        c.Set("captcha_verified", true)
        c.Next()
    }
}
```
