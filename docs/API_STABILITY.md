# API 稳定性承诺

## v1.0.0 起 `/api/v1/*` 接口格式冻结

以下端点的请求体和响应体格式在 v1.x 系列中保持稳定：

| 端点 | 状态 |
|------|------|
| `POST /api/v1/challenge` | 冻结 |
| `POST /api/v1/verify` | 冻结 |
| `POST /api/v1/verify/batch` | 冻结 |
| `POST /api/v1/siteverify` | 冻结 |
| `GET /healthz` | 冻结 |
| `GET /metrics` | Prometheus 格式，指标名称冻结 |
| `GET /sdk/*` | 文件路由冻结，内容随版本更新 |

## 兼容性规则

### 不破坏兼容（v1.x 内允许）
- 响应体新增 optional 字段
- 新增 HTTP 端点
- 新增查询参数（不影响现有请求）
- 新增 Prometheus 指标
- 性能优化 / bug 修复

### 破坏兼容（必须升至 v2.0.0）
- 删除或重命名现有字段
- 修改字段类型
- 修改 HTTP 状态码语义
- 修改 Argon2 参数（影响所有存量 token）
- 删除端点

## SDK 版本

浏览器端 WASM 通过 `version()` 函数返回版本号。
当服务端升级到新的 major 版本时，旧 SDK 的 WASM 将无法通过验证。
建议在 SDK `<script>` URL 中包含版本号以便客户端缓存管理：

```html
<script src="https://captcha.example.com/sdk/pow-captcha.js?v=1.0.0"></script>
```
