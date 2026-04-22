# 升级指南

## 算法参数变更

PoW CAPTCHA 的核心安全性依赖于 Argon2id 参数和 SHA-256 双阶段协议的一致性。
**客户端 WASM 和服务端必须使用完全相同的参数**，否则验证会失败。

### 当前参数（v1.0.0）

| 参数 | 值 |
|------|-----|
| 算法 | Argon2id v0x13 |
| m_cost | 4096 KiB (4 MiB) |
| t_cost | 1 |
| p_cost | 1 |
| 输出长度 | 32 字节 |
| Phase 2 | SHA-256(base_hash \|\| nonce_le_8B) |

参数定义在 `crates/captcha-core/src/pow.rs` 第 20-23 行。

### 升级步骤

如果需要调整 Argon2 参数（例如 `m_cost` 从 4096 升到 8192）：

1. **修改代码**
   ```rust
   // crates/captcha-core/src/pow.rs
   const M_COST: u32 = 8192; // 改为新值
   ```

2. **重新构建全部产物**
   ```bash
   bash scripts/build-all.sh
   ```
   这会同时重建 WASM（嵌入客户端）和服务端二进制，保证算法一致。

3. **灰度发布**
   - 参数变更会导致**所有存量 challenge 和 token 立即失效**
   - 建议采用蓝绿发布：
     1. 部署新版本到备用节点
     2. 切流量到新节点
     3. 等待旧 token TTL 过期（默认 5 分钟）
     4. 下线旧节点

4. **不支持混合版本**
   - 不能同时运行新旧参数的服务端
   - 客户端 WASM 和服务端必须同步更新

### `diff` 调整（无需重新部署）

`diff` 参数不影响算法本身，只影响难度判定。通过配置即可调整：

```toml
# captcha.toml
[[sites]]
key = "pk_example"
diff = 20  # 从 18 改为 20
```

保存后 30 秒内自动热重载生效，无需重启。

### API 版本兼容

v1.0.0 冻结 `/api/v1/*` 的请求/响应格式：
- 新增字段向后兼容（新增 optional 字段不破坏旧客户端）
- 删除/改名字段将通过 `/api/v2/*` 引入
- 客户端 SDK 通过 `version()` 函数返回 WASM 版本号，可用于前端校验
