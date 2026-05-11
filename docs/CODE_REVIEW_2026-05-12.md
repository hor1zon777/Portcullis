# 代码审查报告 — Portcullis PoW CAPTCHA

- **审查日期**：2026-05-12
- **审查范围**：`crates/captcha-core`、`crates/captcha-server`、`crates/captcha-wasm`、`sdk/src`、`admin-ui/src`、`examples/backend-node`
- **当前分支**：`main`（最近提交 `3e0ce3f`）
- **结论**：整体设计扎实，密码学使用规范、签名/防重放/双 key 轮换都到位。但有 **2 个严重 bug** 影响限流与批量接口的安全语义，另有若干并发竞态与错误处理疏漏需要修复。

---

## 严重程度图例

| 等级 | 含义 |
| --- | --- |
| 🔴 CRITICAL | 在生产环境下可导致绕过限流 / 静默失败 / 拒绝服务 |
| 🟠 HIGH | 在特定时序下可导致 panic、数据丢失或可观察的安全副作用 |
| 🟡 MEDIUM | 行为不一致、维护性差、潜在被利用条件需配合其他漏洞 |
| 🟢 LOW | 体验、健壮性、规范性问题 |

---

## 🔴 CRITICAL

### C-1：`IpRateLimiter::check` 清理逻辑反转，导致限流被绕过

`crates/captcha-server/src/rate_limit.rs:41-45`

```rust
if self.limiters.len() > 50_000 {
    let before = self.limiters.len();
    self.limiters.retain(|_, limiter| limiter.check().is_err());
    tracing::debug!("限流器清理：{} → {}", before, self.limiters.len());
}
```

**问题**：
1. `retain(|_, l| l.check().is_err())` 表示 **保留** "当前没有可用令牌"的桶（即正在被限流的恶意 IP），**删除** 当前有令牌的桶（正常用户）。语义和注释 "令牌桶满的条目可安全移除" 相反。
2. `limiter.check()` 这一次调用会 **消耗一个令牌**。也就是说每次容量保护触发，所有未被限流的 IP 都会被白白扣一个 token。
3. 攻击者可以利用此逻辑：先用 50K+ 不同 IP 撑爆 DashMap，触发清理后他们已经处于"限流中"状态的桶得以保留；而正常用户的桶被删除——下次请求时新建一个**满桶**，等效"重置"了对正常用户的限流计数器。

**最小修复**：
```rust
self.limiters
    .retain(|_, limiter| limiter.check_n(NonZeroU32::new(1).unwrap()).is_err());
```
或换成时间戳维度（governor 的 `RateLimiter::clock().now()` 与桶最后访问时间对比）来淘汰真正"空闲"的 IP。**绝不能在清理中再次调用副作用 `check()`**。

---

### C-2：`/api/v1/verify/batch` 绕过日志、风控、IP 黑名单

`crates/captcha-server/src/routes/verify.rs:197-232`

`verify_batch` 内部循环对每个 item 调用 `do_verify`，但与单条路径 `verify(...)` 相比缺少全部副作用：

| 副作用 | `verify` | `verify_batch` |
| --- | --- | --- |
| `state.request_log.inc()` | ✅ | ❌ |
| `crate::db::insert_log(...)` | ✅ | ❌ |
| `state.risk.record_verify(...)` | ✅ | ❌ |
| 错误码标准化 | `AppError` | `format!("{e:?}")` 暴露 Debug |
| 客户端 IP 风控统计 | ✅ | ❌ |

**影响**：
- 攻击者可以批量提交 20 条伪造的 challenge，所有失败请求**不会**计入风控滑动窗口，从而**绕过动态难度提升**。
- 管理面板的日志面板 / 审计 / 风控 IP 排名都看不到批量调用流量。
- Prometheus 指标 `captcha_verify_fail_total` 在批量路径下虽然记了，但缺 IP 维度的风控统计。
- `Some(format!("{e:?}"))` 把 `AppError::Internal("系统时钟异常")` 这类内部错误以 Debug 格式直接写进响应体。

**修复方向**：把单条路径里的 `started`/`client_ip`/日志写入/风控更新逻辑抽成 helper，让 `verify_batch` 复用；改成 `format!("{e}")`（Display）或者直接映射成稳定的错误码字符串。

---

## 🟠 HIGH

### H-1：`MemoryStore::cleanup_expired` 在并发插入下减法下溢 panic

`crates/captcha-server/src/store/memory.rs:48-55`

```rust
let before_c = self.challenges_used.len();
self.challenges_used.retain(|_, exp| *exp > now);
let before_t = self.tokens_used.len();
self.tokens_used.retain(|_, exp| *exp > now);
(before_c - self.challenges_used.len()) + (before_t - self.tokens_used.len())
```

`DashMap::len()` 没有快照语义。在 `before_c` 取值后、`retain` 期间，若另一个线程并发 `insert`，`retain` 可能保留更多条目，导致 `before_c - self.challenges_used.len()` 在 `usize` 上下溢 panic。

后台清理任务每 30 秒触发，verify 路径每次都会 `enforce_capacity` 触发同样的清理，并发非常容易满足条件。

**修复**：用 `saturating_sub` 或者在 retain 中累加被移除的数量。

```rust
let removed_c = AtomicUsize::new(0);
self.challenges_used.retain(|_, exp| {
    if *exp <= now { removed_c.fetch_add(1, Ordering::Relaxed); false } else { true }
});
```

---

### H-2：DB 写入采用 fire-and-forget `tokio::task::spawn_blocking`，错误被吞

`crates/captcha-server/src/routes/verify.rs:61, 113-115`
`crates/captcha-server/src/routes/siteverify.rs:115-122`
`crates/captcha-server/src/admin/audit.rs:94-104`

```rust
tokio::task::spawn_blocking(move || crate::db::insert_log(&db, &log_entry));
```

这是"扔出去就不管"模式：
1. `JoinHandle` 没有保留，DB 写入失败时只有 db.rs 内部的 `tracing::warn!`，主流程无任何感知。
2. 服务在 spawn 之后立即响应客户端 `success: true` —— 但**审计记录或防重放 DB 行可能还没落盘**。短时间内 crash/重启会丢失这些事件。
3. `tokio::task::spawn_blocking` 在 tokio 阻塞线程池满了之后会阻塞主调度器；这里没有任何背压。

**修复方向**：
- 关键写入（防重放 nonce）改成 await 并把失败映射到 `AppError::Internal`，避免内存认为已经标记但 DB 没有；
- 日志/审计仍可异步，但应该把 handle 推到一个 mpsc channel + 后台 worker 集中提交，便于背压和监控。

---

### H-3：内存防重放与 DB 防重放的时序窗口

`crates/captcha-server/src/routes/verify.rs:102-116`

`mark_challenge_used` 先在 `MemoryStore` 内插入并立刻返回，DB 写入通过 `spawn_blocking` 异步执行。窗口内服务进程 crash → 重启后 `replay_nonces` 表里没有这条记录 → **同一 challenge_id + sig + nonce 可以被重放一次**。

虽然 challenge TTL 默认只有 120s，但配合 H-2 的"丢失"特征，这是 [防重放保证的可观察缺口]。

**修复**：在标记内存 used 之前先同步落盘 DB，或采用 SQLite 的 `INSERT OR IGNORE`+返回值作为唯一可信来源（性能确实会差，要权衡）。

---

### H-4：`AppState::reload_config` 不是原子操作，risk/config 之间存在间隙

`crates/captcha-server/src/state.rs:42-52`

```rust
self.config.store(Arc::new(merged));
self.risk.write().await.update_config(risk_cfg);
```

`config` 先被替换，`risk` 后被更新。在两步中间：
- 新的 config 已经生效（包含新的 `dynamic_diff_max_increase` 等），
- 但 `risk` 还在用旧配置（旧的 blocked/allowed CIDR、旧的窗口大小）。

`/challenge` handler 先 `state.config.load()` 拿新阈值，又拿旧 risk 黑白名单。短时间内规则错配。不会 panic，但会出现 "为什么我刚加的黑名单还没生效" 的诡异行为，且热重载场景下被合法用户先到的请求 race 概率较大。

**修复**：把 `config` 和 `risk` 用同一把 RwLock 包起来一起替换，或者让 `risk` 把 hot-reload 信号直接订阅 `config`。

---

### H-5：admin token 比较短路时泄露长度

`crates/captcha-server/src/admin/auth.rs:44-47`

```rust
let matches = match &provided {
    Some(t) if t.len() == expected.len() => t.as_bytes().ct_eq(expected.as_bytes()).into(),
    _ => false,
};
```

`ct_eq` 只在长度一致时进入，长度不同直接返回 false。这暴露了 admin token 的精确长度（虽然攻击者随后还要面对 30 次失败 = 15 分钟 ban），仍是一个可观察的侧信道。

**修复**：始终走 `ct_eq`——长度不一致时先 hash 两边再比较，或者填充到固定长度。一个简单做法是直接对两边都做 sha256 后 `ct_eq`。

---

### H-6：`update_site_fields` 多个独立 UPDATE 不在事务里

`crates/captcha-server/src/db.rs:312-397`

PUT `/admin/api/sites/:key` 每个字段都触发一条单独 UPDATE，没有 `BEGIN/COMMIT`。
- 中途崩溃 → 部分字段已写、部分未写、`updated_at` 来回跳。
- 上层 handler 还会再 `reload_config` 把 ArcSwap 替换。如果 DB 写入和 ArcSwap 不一致，重启后从 DB 加载将与内存不同。

**修复**：把所有字段合成单条 UPDATE，或者用 rusqlite 的 `Transaction`。

---

## 🟡 MEDIUM

### M-1：管理员 webhook 缺 SSRF 防护

`crates/captcha-server/src/admin/webhook.rs`

`admin_webhook_url` 由管理员配置，但 reqwest 默认 follow redirect。恶意 admin（或 admin token 泄露后）可以让服务端访问内网 metadata 服务（`169.254.169.254`、`127.0.0.1:*`、kube-apiserver 等）。

**修复**：
- 禁用重定向：`Client::builder().redirect(reqwest::redirect::Policy::none())`；
- DNS 解析后白名单（拒绝私有/loopback/链路本地段）；
- 文档明确警告 webhook URL 是高敏配置。

---

### M-2：服务端不校验 site.diff 上限

`crates/captcha-server/src/routes/challenge.rs:80`
`crates/captcha-server/src/admin/handlers.rs:88`

```rust
let effective_diff = site.diff.saturating_add(extra_diff);
```

`diff` 是 `u8`，admin handler `validate_argon2_params` 只校验 argon2 参数。管理员把 diff 设到 60 后，客户端永远求解不出来 —— 整个站点的验证码被静默废掉，且 admin UI 客户端 max=28 校验是软校验（POST 直接构造 60 仍然能写入）。

**修复**：在 `validate_argon2_params` 旁边加 `validate_diff_range`，拒绝 < 8 或 > 28。

---

### M-3：`CorsLayer::permissive()` 应用到所有路由，包括 `/admin/api/*`

`crates/captcha-server/src/lib.rs:38`

permissive 等于 `Access-Control-Allow-Origin: *`、放通所有 method/header。Admin 路由因为 Bearer token 不在 cookie，且需要主动加 `authorization` header，跨域读取受 SOP 保护，**目前并非可利用的 XSS gadget**。但是：
- /admin/api/* 应该收紧到固定 admin-ui origin 集，避免未来某个新 endpoint 上 cookie auth 时引爆 CSRF；
- 安全审计里这是常被打的低悬果。

**修复**：admin 子路由用 `Router::new(...).layer(CorsLayer::new().allow_origin(...))` 覆盖，业务 API 保留 permissive。

---

### M-4：SDK widget 主线程跑 Argon2，阻塞 UI

`sdk/src/widget.ts:295-298`

```ts
await new Promise((r) => setTimeout(r, 50));
const solver = wasm.create_solver(payloadJson, toBigIntSafe(this.opts.maxIters));
```

`create_solver` 内会 **同步** 跑一次 Argon2id（默认 19456 KiB / t=2）。在中端机器上约 30-80ms，但低端 Android 上可达 300-800ms，期间整页冻结。50ms 的 setTimeout 不够让浏览器渲染完进度文字。

**修复**：
- 真正用 Worker（已有 legacy `solve` API 走 worker，但默认走 chunked 主线程）；
- 或者拆分 Argon2 为多个 `requestIdleCallback` chunk。

---

### M-5：SDK 在 `verify_batch` 错误信息 Debug 格式之外的对外消息

`crates/captcha-server/src/routes/verify.rs:225`

参见 C-2，单独列出是因为它可被普通用户触发，会把内部错误（包括路径/状态/字符串内插）回显。攻击面虽小但属于"信息泄露"分类。

---

### M-6：Prometheus 指标用 `site_key` 作为标签，存在高基数风险

`crates/captcha-server/src/metrics.rs:32-41`

```rust
counter!(label, "site_key" => site_key.to_string()).increment(1);
```

挑战路径已做 `get_site` 校验，site_key 不存在直接 400，但 verify/batch 路径接收任意客户端 JSON 的 `challenge.site_key`。签名验证失败的请求**早于**指标记录，所以非法 site_key 不会创建 series；不过 `record_verify(&site_key, false, ...)` 在 `do_verify` 失败时也会执行 —— 此时 site_key 可能是攻击者构造的任意字符串（虽然签名失败，依然进入 metrics 记录）。

确认看代码：`verify.rs::verify` 在 `do_verify` 失败后仍然调用 `crate::metrics::record_verify(&site_key, success, started)`，其中 `site_key` 取自 `req.challenge.site_key.clone()` —— 攻击者可控。

**修复**：在记录 metrics 前先校验 `site_key in config.sites`，否则归入 `unknown` 标签。

---

### M-7：admin-ui 中的 token 存储在 localStorage

`admin-ui/src/lib/api.ts:1-9`

XSS 拿到的代码可以直接 `localStorage.getItem('captcha_admin_token')`。这是大多数 SPA 的常规做法，但对 admin 这种权限位很高的接口，建议：
- 服务端发 HttpOnly cookie + CSRF token；
- 或至少加上 sessionStorage 并配合短 TTL；
- 启用 CSP 限制脚本来源。

---

## 🟢 LOW

| ID | 文件 | 描述 |
| --- | --- | --- |
| L-1 | `crates/captcha-core/src/challenge.rs:75-79` | `expect("system clock before epoch")` 在系统时钟被回拨到 1970 之前时会 panic。考虑用 `unwrap_or_default()` 让 `is_expired` 返回 true。 |
| L-2 | `crates/captcha-server/src/main.rs:81-82` | `migrate(&db)` 在 main 和 `AppState::new` 里被各调用一次。幂等但冗余。 |
| L-3 | `crates/captcha-server/src/rate_limit.rs:196` | `admin_rate_limiter()` 工厂未被引用，疑似 dead code（admin auth_middleware 自己内部记账，不依赖此 IpRateLimiter）。 |
| L-4 | `sdk/src/types.ts:11-18` | `Challenge` 接口缺 `m_cost / t_cost / p_cost`，TypeScript 消费者拿不到这三个字段（运行时 JSON 仍带）。需要同步类型定义。 |
| L-5 | `sdk/src/widget.ts:13, 73` | `manifestVersionCache` / `wasmCache` 是全局 `Map`，永不清空。多 endpoint 场景下不会泄漏多大内存，但单页面 SPA 反复加载销毁也无法回收。 |
| L-6 | `sdk/src/widget.ts:101-118` | `loadWasmViaScript` 假设 `globalThis.wasm_bindgen` 存在，但 wasm-pack `web` target 的输出不挂全局，这条 fallback 在该 target 下永远失败。检查 `sdk/pkg/captcha_wasm.js` 实际形态。 |
| L-7 | `sdk/src/auto-mount.ts:96-98` | `(window as any)[callbackName](token)` 允许通过 `data-callback` 调任意全局函数。前提是攻击者已经能写 HTML（已 XSS），但仍应文档化警示。 |
| L-8 | `admin-ui/src/lib/utils.ts:46` | `IP_RE` 不校验 v4 段值范围（接受 `999.999.999.999`），也不严格判 IPv6。建议改用 `URL` 解析或 `ipaddr.js`。 |
| L-9 | `crates/captcha-server/src/admin/handlers.rs:191-228` | `delete_site` 删除时不停用现有 token，已发放且未过期的 token 仍能被 siteverify 接受（因为 verify_full 不校验 site 仍在 sites）。文档可能需要说明。 |
| L-10 | `crates/captcha-server/src/db.rs:30-35` | `Connection::open(...).expect(...)` 在 Windows 上若 WAL 创建失败（罕见但存在）会 panic。生产环境上建议 fall back 到 `DELETE` journal_mode。 |
| L-11 | `crates/captcha-server/src/admin/handlers.rs:312-321` | 调 `crate::db::update_site_fields` 时传入 `req.diff` 等原始 `Option<u8>`；handler 里**已经**把字段写入 site，DB 失败不会回滚 ArcSwap。配合 H-6 修复。 |
| L-12 | `crates/captcha-wasm/src/lib.rs:60` | `attempts: nonce + 1` 在 nonce 接近 `u64::MAX` 时溢出。实际不可能达到，但 `wrapping_add(1)` 更稳。 |
| L-13 | `admin-ui/src/pages/Sites.tsx:106` | `form.origins.split(',').map(s => s.trim()).filter(Boolean)` 不校验 origin 格式（应是 `https?://host[:port]`）。误输导致 origin 永远不匹配。 |
| L-14 | `crates/captcha-server/src/risk.rs:120-130` | `cleanup_stale` 用 `Instant::now() - Duration::from_secs(600)`；在系统启动头 600 秒会下溢 panic。Rust 的 `Instant - Duration` 在不足时 panic（debug）/返回旧值（release）。建议 `checked_sub`。 |

---

## 测试与质量观察

- **测试覆盖率较高**：`tests/integration.rs` 涵盖 happy path、replay、签名篡改、IP/UA 绑定、双 key 轮换、admin audit、admin ban 等核心场景，整体很扎实。
- **缺失的覆盖点**：
  - `verify_batch` 路径完全没有集成测试，正好与 C-2 互为印证；
  - `cleanup_expired` 并发场景没有 stress 测试；
  - `IpRateLimiter` 容量保护那段没单测；
  - `webhook.rs` 没有 mock HTTP 服务端的集成测试，重定向/超时分支无验证。
- **CI**：从仓库结构看有 `.github/`，但本次审查未运行 `cargo test` / `pnpm build`，建议这次修复后跑一次完整 CI 留档。

---

## 修复优先级建议

1. **本周内合并**：C-1、C-2、H-1、H-2、H-3。这五个都是"语义级"问题，会让安全特性失效。
2. **下个 minor**：H-4、H-5、H-6、M-1、M-2、M-6。这些是稳定性 + 完备性提升。
3. **长期 backlog**：M-3 ~ M-7、L 系列。建议合并到同一个 hardening sprint。
4. **新增测试**：每个 C/H 修复都伴随一条回归测试，特别是 `verify_batch` 的日志/风控副作用。

---

## 整体评价

- 加密、签名、双 key 轮换、Ed25519 manifest 签名、admin 审计这些设计层面的东西都做得很到位，没有发现密码学误用。
- 主要风险集中在 **并发清理 / 异步副作用 / 批量路径副作用缺失** 三类工程问题。
- 修复路径都比较短（5-50 行级），不需要大改架构。
- 建议沿 C-1/C-2 排查类似"清理 + 批量 + 副作用"的代码模式，看是否还有同类隐藏 bug。
