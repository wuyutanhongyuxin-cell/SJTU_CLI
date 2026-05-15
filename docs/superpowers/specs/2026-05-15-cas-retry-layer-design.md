# CAS retry 层 follow-up — Design Spec

> **状态**：Draft → 待用户过审 → 转 writing-plans
> **日期**：2026-05-15
> **范围**：在 `src/auth/cas/` 新增通用 retry helper，把 T9 真机暴露的"jwc sub_session 客户端 fresh 但 ZF 服务端 timeout"盲区根治；本轮接入只覆盖 jwc 5 个命令背后 9 个 handler call site，elec/services/jwbmessage/canvas_video 留作未来扩展（位置已为通用层）
> **复用基线**：`auth::cas::cas_login` (S2 / staleness-fix 11e1917) / `cookies::clear_sub_session` / `commands/canvas_video/retry.rs::with_token_refresh`（同构 pattern 先例，49 行已 production 验证）/ `jwc/http.rs:142-148` 已有 staleness detect

---

## 1. 背景与现状

### 1.1 T9 真机暴露的盲区

`tasks/lessons.md` 2026-05-15 段已沉淀：

- 现象：`sjtu jwc calendar` 首次跑 `eventCount=0`，warnings 含 ZF redirect 到 `login_slogin.html`
- 客户端 `sub_sessions/jwc.json` `captured_at` 在 30 天窗口内（`cache_is_fresh ✓`），但 ZF 服务端 session 已 timeout（ZF 默认 30 分钟无活动）
- 临时修复（T9 内）：删 `%APPDATA%\sjtu\sjtu-cli\config\sub_sessions\jwc.json` 一个文件 → 让 `Client::connect` 用还有效的主 session 走 CAS 自动跳转刷新；不动主 session 避免用户重新扫码
- 根因：staleness-fix 只在客户端 `captured_at` 上判断；**没在 ZF redirect 检测路径上挂自动刷新** —— 服务端 TTL 30 分钟级，远短于客户端 cache TTL 30 天，必然漏

教训定调：staleness 双轨 — cache freshness check + redirect detect retry。本 spec 解决后者。

### 1.2 现有 codebase 基线

| 模块 | 文件 | 状态 |
|---|---|---|
| CAS 主入口 | `src/auth/cas/mod.rs:44 cas_login(name, target_url)` | ✓ 有 cache hit + `captured_at` staleness 判定 |
| 主 session 失效兜底 | `src/auth/cas/mod.rs:77` 抛 `SubSystemUnreachable("cas", "请先 sjtu logout && sjtu login")` | ✓ 已有友好提示 |
| Cookie I/O | `src/cookies/io.rs:78 clear_sub_session(name)` | ✓ 已 export |
| jwc staleness detect | `src/apps/jwc/http.rs:142-148` final_url 含 jaccount 或 body 起始是 HTML → `UpstreamError("session 已失效...")` | ✗ detect 后**直接抛错让用户手动重 login**，没自动刷新 |
| 同构 retry pattern | `src/commands/canvas_video/retry.rs::with_token_refresh` | ✓ 已 production 验证，49 行；本 spec 同构复用 |
| jwc Client 入口 | `src/apps/jwc/api/mod.rs:71 Client::connect()` | ✓ 已有；本 spec 拆出 `Client::from_session(sess)` |

### 1.3 联网验证综合（2026-05-15）

4 维 WebSearch 综合 Rust 2026 业界 idiomatic vs 本项目约束：

| 维度 | 业界 idiomatic 2026 | 本项目选 |
|---|---|---|
| Retry 层载体 | `reqwest-middleware` + `RetryableStrategy` trait impl | 手卷闭包 helper |
| 错误信号 | `thiserror` variant + 模式匹配（**反对**字符串匹配） | `SubSessionStale(&str)` variant + `downcast_ref` ✓ idiomatic |
| Retry 时机 | failure-driven (lazy) | failure-driven (lazy) ✓ |
| 测试 | mockito / wiremock round-trip | mockito round-trip ✓ |

**为何不选业界 idiomatic middleware**（理由必须 doc 进 retry.rs 顶部 module comment）：

1. 违反 CLAUDE.md "不引入新依赖"硬约束（+2 crate：`reqwest-middleware` + `reqwest-retry`）
2. 改造面 ×6 子系统（jwc/elec/services/jwbmessage/shuiyuan/canvas 全部用裸 `reqwest::Client`，迁 `ClientWithMiddleware` 是 sweeping refactor）
3. Stateful side-effect（清缓存文件 + 重 CAS 跳转）在 stateless `RetryableStrategy` trait 里别扭
4. 本轮 scope = 1 个子系统接入，middleware 是给 N 个 client 复用的方案，1 处手卷更轻

`SubSessionStale` variant 选择正中业界 idiomatic（"For retry patterns specifically, use thiserror to define specific variants rather than relying on downcasting with anyhow's error message string"）。

---

## 2. Goals / Non-Goals

### Goals

- G1 — 用户跑 `sjtu jwc <any>` 遇到 ZF 服务端 timeout 时**自动透明刷新**，不再需要手动 `sjtu logout && sjtu login` 或手删 `sub_sessions/jwc.json`
- G2 — 通用层 `src/auth/cas/retry.rs::with_cas_refresh<F,Fut,T>(name, target_url, op)` 复用 canvas_video 同构 pattern，future 扩 elec/services/jwbmessage 是"加一处 call site"级别（不本轮做）
- G3 — `SubSessionStale(&'static str)` 强类型 retry 信号 variant；`is_sub_session_stale(&anyhow::Error)` 靠 `downcast_ref` 判定（**反对**字符串匹配，避免 error message 文案改时 retry 静默退化）
- G4 — `jwc/http.rs:142-148` 现有 staleness detect 从 `UpstreamError("session 已失效")` 改抛 `SubSessionStale("jwc")`；保持 envelope `error.code = session_expired` 行为不变
- G5 — max retry = 1，无 backoff（同 canvas_video）；retry 失败后 cas_login 内置兜底（主 session 也挂时抛 `SubSystemUnreachable("cas", "...请先 sjtu logout && sjtu login")`）
- G6 — 单测：mockito 模拟 op 第一次返 SubSessionStale + 第二次返 OK round-trip；retry 不触发的边界（NetworkError）
- G7 — 真机 staleness 复现 smoke：删 jwc.json 后跑命令 → 自动恢复；或等 30 分钟服务端 timeout → 跑命令 → 自动恢复

### Non-Goals

- **NG1**：本轮不接 elec/services/jwbmessage —— 没真机暴露 staleness，且它们的 `http.rs` 缺 HTML 兜底信号 detect，要接需补 detect 层；留作独立 follow-up
- **NG2**：不接 canvas_video —— 它有独立的 `with_token_refresh`（LTI launch + token 体系），跟 CAS 路径不同，本轮不动
- **NG3**：不引入 `reqwest-middleware` / `reqwest-retry` —— 见 §1.3 trade-off
- **NG4**：不做 backoff / exponential retry —— max retry = 1 已足；ZF stale 信号是确定性的，不是 transient 网络抖动
- **NG5**：不包写操作（POST/PUT/DELETE）—— jwc / i.sjtu 硬红线本来就只读；retry helper 不做幂等性判定，约定上层只在 GET 端点用
- **NG6**：不抽 `pub trait CasStaleSignal { fn is_stale(&self) -> bool; }` —— YAGNI，1 个 variant 不需要 trait；未来扩第二个信号时再抽

---

## 3. 文件布局

5 modify + 1 new + 2 测试，全部 < 200 行：

```
src/
├── error.rs                              # +5 行：加 SubSessionStale variant + code() arm
├── auth/cas/
│   ├── mod.rs                            # +2 行：pub mod retry + re-export with_cas_refresh
│   └── retry.rs (NEW)                    # ~70 行：with_cas_refresh + is_sub_session_stale + 3 tests
└── apps/jwc/
    ├── http.rs:142-148                   # ±3 行：UpstreamError → SubSessionStale("jwc")
    └── api/mod.rs                        # +10 行：拆 Client::from_session(sess) + connect() 改调
└── commands/jwc/
    ├── handlers.rs                       # ±20 行：4 个 handler (grades/schedule/gpa/exams) 改 with_cas_refresh 闭包
    ├── gpa_handlers.rs                   # ±5 行：cmd_gpa_by_semester 改
    ├── schedule_handlers.rs              # ±15 行：today/week/next 3 handler 改
    └── ical/handler.rs                   # ±5 行：cmd_calendar 改
```

预估总 ~135 行新增 + ~50 行迁移；每文件均在 200 行硬限内。**注意 `jwc/http.rs` 现 192 行，紧贴 200 限**：本期改动是同行替换，不增行；若 +3 行会触限，task 内做行数 check。

---

## 4. 架构总览

### 4.1 API 形态

```rust
// src/auth/cas/retry.rs

use std::future::Future;
use anyhow::Result;
use tracing::warn;

use crate::cookies::{clear_sub_session, Session};
use crate::error::SjtuCliError;
use super::cas_login;

/// CAS 子系统服务端 stale-detect + auto-refresh retry helper。
///
/// 用法：
/// ```ignore
/// let grades = with_cas_refresh("jwc", LOGIN_URL, |session| async move {
///     let client = Client::from_session(session)?;
///     client.grades(xnm, xqm, page, limit).await
/// }).await?;
/// ```
///
/// 工作流程：
/// 1. cas_login 拿 session（命中 cache 则直接返）
/// 2. 调 op(session) 跑业务
/// 3. 若返 SubSessionStale → clear_sub_session(name) + 重 cas_login → op(session2)
/// 4. 若仍返 SubSessionStale 或主 session 也挂 → 原样上抛
///
/// 仅适用于 **GET only / 幂等只读**操作（CLAUDE.md i.sjtu 硬红线）；
/// 上层不得用此 helper 包 POST/PUT/DELETE。
pub async fn with_cas_refresh<F, Fut, T>(
    name: &'static str,
    target_url: &str,
    op: F,
) -> Result<T>
where
    F: Fn(Session) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let r = cas_login(name, target_url).await?;
    match op(r.session.clone()).await {
        Ok(v) => Ok(v),
        Err(e) if is_sub_session_stale(&e) => {
            warn!(name, error = %e, "sub_session 服务端 stale，清缓存重做 CAS");
            clear_sub_session(name)?;
            let r2 = cas_login(name, target_url).await?;
            op(r2.session).await
        }
        Err(e) => Err(e),
    }
}

/// downcast 判定 retry 信号；不依赖错误 message 字符串。
fn is_sub_session_stale(e: &anyhow::Error) -> bool {
    e.downcast_ref::<SjtuCliError>()
        .map(|err| matches!(err, SjtuCliError::SubSessionStale(_)))
        .unwrap_or(false)
}
```

### 4.2 Error variant 新增

```rust
// src/error.rs

#[derive(Debug, Error)]
pub enum SjtuCliError {
    // ... 既有 variant 不变 ...

    /// 子系统 sub_session 客户端 cookie 仍 fresh 但服务端已 timeout
    /// （ZF 30 分钟无活动 / OAuth2 短 TTL 等）。`with_cas_refresh` 捕获此 variant
    /// 触发 clear_sub_session + 重 CAS。`&'static str` 是子系统 name（"jwc" 等）。
    #[error("子系统 `{0}` session 已被服务端失效")]
    SubSessionStale(&'static str),
}

impl SjtuCliError {
    pub fn code(&self) -> &'static str {
        match self {
            // ... 既有 arm 不变 ...
            Self::SubSessionStale(_) => "session_expired",
        }
    }
}
```

`code()` 映射到 `session_expired` —— 跟 `SessionExpired` 同 envelope code，但用户**不应该看到这个错** —— retry 成功 = 静默；retry 失败 = 主 session 也挂，由 cas_login 抛 `SubSystemUnreachable` 接管。`SubSessionStale` 上抛到用户层是兜底（如 retry 内 clear_sub_session 失败），envelope code 保持一致语义。

### 4.3 jwc/http.rs detect 改造

```rust
// src/apps/jwc/http.rs:142-148 改动前
if final_url.contains("jaccount.sjtu.edu.cn")
    || body.trim_start().starts_with("<!DOCTYPE")
    || body.trim_start().starts_with("<html")
{
    return Err(SjtuCliError::UpstreamError(format!(
        "{label} session 已失效（被打回登录页 final_url={final_url}）；请 `sjtu logout` 后重新 `sjtu login`"
    )).into());
}

// 改动后
if final_url.contains("jaccount.sjtu.edu.cn")
    || body.trim_start().starts_with("<!DOCTYPE")
    || body.trim_start().starts_with("<html")
{
    tracing::debug!(label, %final_url, "ZF 服务端 stale，触发 SubSessionStale");
    return Err(SjtuCliError::SubSessionStale("jwc").into());
}
```

`final_url` 信息进 tracing debug（不丢可观测性），错误本身用强类型 variant。

### 4.4 Client::from_session 拆分

```rust
// src/apps/jwc/api/mod.rs 改造

impl Client {
    /// 便利门面：cas_login + from_session。
    /// **不参与 with_cas_refresh 的场景下用**（如 status / debug 命令）；
    /// 参与 retry 的命令应直接用 with_cas_refresh + Client::from_session。
    pub async fn connect() -> Result<Self> {
        let r = cas_login("jwc", LOGIN_URL).await?;
        let client = Self::from_session(r.session)?;
        Ok(Self {
            login: LoginMeta {
                from_cache: r.from_cache,
                elapsed_ms: r.elapsed_ms,
                final_url: r.final_url,
            },
            ..client
        })
    }

    /// 从已有 session 构造 Client（cookie jar + http builder）。
    /// 主要给 with_cas_refresh 的闭包内调用（CAS 已在 helper 外做，不重复）。
    pub fn from_session(session: Session) -> Result<Self> {
        let http = build_http_client(&session)?;
        Ok(Self {
            http,
            throttle: Arc::new(Throttle::new()),
            time_counter: AtomicU32::new(0),
            visited_sp: Mutex::new(HashSet::new()),
            login: LoginMeta::default(),  // from_session 不知道 CAS 元数据
        })
    }
}

// LoginMeta 加 Default impl（from_session 用）
impl Default for LoginMeta {
    fn default() -> Self {
        Self { from_cache: false, elapsed_ms: 0, final_url: String::new() }
    }
}
```

### 4.5 Handler 改造样板（以 cmd_grades 为例）

```rust
// src/commands/jwc/handlers.rs 改造样板

use crate::auth::cas::with_cas_refresh;
use crate::apps::jwc::api::LOGIN_URL;  // 需 pub(crate) 暴露
use crate::apps::jwc::Client;

pub async fn cmd_grades(
    xnm: Option<String>,
    xqm: Option<String>,
    page: u32,
    limit: u32,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let xnm_s = xnm.clone();  // closure 跨多个 await 拿不到借用，先 clone
    let xqm_s = xqm.clone();
    let env_resp = with_cas_refresh("jwc", LOGIN_URL, |session| {
        let xnm = xnm_s.clone();
        let xqm = xqm_s.clone();
        async move {
            let client = Client::from_session(session)?;
            client.grades(xnm.as_deref(), xqm.as_deref(), page, limit).await
        }
    }).await?;
    // ... 后续 render 不变
}
```

9 个 handler 全套同构改法。`LOGIN_URL` 当前是 `pub(super)`，需提到 `pub(crate)`。

---

## 5. Data flow（3 路径）

### 5.1 正常路径（cache hit + 服务端仍 fresh）

```
Handler → with_cas_refresh("jwc", LOGIN_URL, op)
       → cas_login("jwc", LOGIN_URL)             # cache hit, ~5ms
       → op(session) = Client::from_session + client.grades(...)
       → POST i.sjtu/.../N305005 → 200 JSON
       → 返回 Ok(grades)
```

### 5.2 服务端 stale 路径（首次失败 + 自动恢复）

```
Handler → with_cas_refresh("jwc", LOGIN_URL, op)
       → cas_login("jwc", LOGIN_URL)             # cache hit, captured_at fresh
       → op(session) → POST i.sjtu/.../N305005
       → final_url = .../login_slogin.html (ZF 服务端 stale)
       → jwc/http.rs detect → Err(SubSessionStale("jwc"))
       ← retry 层捕获 → clear_sub_session("jwc")
       → cas_login("jwc", LOGIN_URL)             # cache miss, 走 CAS 链 ~2-3s
       → op(session2) → POST → 200 JSON
       → 返回 Ok(grades)
```

用户视角：命令多耗 ~2-3s（CAS 重做），输出正常 envelope；retry 进 tracing warn 但不污染 stdout / stderr。

### 5.3 主 session 也挂路径（兜底）

```
Handler → with_cas_refresh(...)
       → cas_login(...) → cache hit
       → op(session) → SubSessionStale
       ← clear_sub_session
       → cas_login("jwc", LOGIN_URL)             # cache miss
       → follow_redirect_chain 最终停在 jaccount.sjtu.edu.cn
       → cas/mod.rs:77 抛 SubSystemUnreachable("cas", "请先 sjtu logout && sjtu login")
       ← retry 层原样上抛
       → 用户看到 envelope error.code=sub_system_unreachable + 友好行动项
```

cas_login 已有兜底，retry 层不需要额外处理。

---

## 6. 测试策略

### 6.1 单测（retry.rs 内 ~50 行）

- **T_unit_1 retry_on_stale_then_ok**：op 第一次返 `SubSessionStale("jwc")`，第二次返 Ok(42) → 验证 helper 返 42 + clear_sub_session 调用 1 次（mock 文件系统 / 用 tempdir）
- **T_unit_2 no_retry_on_other_error**：op 第一次返 `NetworkError(...)` → 验证 helper 不触发 retry，原样返 NetworkError + clear_sub_session 未调用
- **T_unit_3 retry_then_fail**：op 第一次返 SubSessionStale，第二次返 NetworkError → helper 返第二次错（NetworkError）

注意：cas_login 不能在 unit test 里真跑（需要主 session 文件）；T_unit_1/2/3 把 cas_login 部分 stub 掉，只测 `op + clear_sub_session` 逻辑。要么把 retry 核心提取成 `fn with_refresh_inner<F,Fut,T>(op, refresh_fn)` 把 refresh 函数注入（更 testable）。**Plan 阶段决定**：是否抽 refresh 注入，或者用 `#[cfg(test)] mod tests` + mock cas_login 函数指针。

### 6.2 集成测（一个 jwc handler 走 round-trip）

`tests/jwc_retry_integration.rs`（新文件 ~80 行）：

- mockito 启 server，hijack `i.sjtu.edu.cn` (需 reqwest base URL 注入；现 http.rs 用 `const BASE` 写死，集成测要嵌一个 base URL override 钩子)
- 第一次 POST → 返 HTML 登录页（模拟 ZF stale）
- 第二次 POST → 返合法 JSON
- 验证 handler 透明返 JSON envelope，第一次失败不上抛

**注意**：现有 `BASE` 是 const，需改成 `pub(super) fn base() -> &'static str` 读 env var 兜底，仅测试模式生效（CLAUDE.md 不污染 production path）。Plan 阶段细化。

### 6.3 真机 smoke

- **CP-CR-1**：删 `sub_sessions/jwc.json` → 跑 `sjtu jwc grades --xnm 2025 --xqm 12` → 验证 envelope 正常 + tracing 看到 "sub_session 服务端 stale，清缓存重做 CAS"（但这是 client-side stale，不是 server-side）
- **CP-CR-2**（真正盲区复现）：登录后等 ≥30 分钟不动 → 跑 `sjtu jwc grades` → 第一次本应失败但自动 retry 成功
- **CP-CR-3**：主 session 失效（手动改 session.json 的 JAAuthCookie 值损坏）→ 跑命令 → 验证抛 SubSystemUnreachable + 友好提示

---

## 7. Open Questions

- **OQ1**（plan 阶段定）：mockito 集成测要 hijack `BASE` —— 改 const 为 fn + env var，还是给 Client 加 `with_base(url)` 注入？倾向后者（test-only API，不动 production）
- **OQ2**（plan 阶段定）：retry.rs 单测如何 stub cas_login —— 抽 `with_refresh_inner(op, refresh: FnMut())` 注入 refresh，还是用 `#[cfg(test)]` mock？倾向前者（更干净，refresh_fn 在 test 里返 ok session）

两个 OQ 都不阻塞 spec 通过；plan T0/T1 阶段任选其一固定。

---

## 8. 不引入 reqwest-middleware 的 doc note（写入 retry.rs 顶部）

```rust
//! CAS 子系统 stale-detect + auto-refresh retry 通用 helper。
//!
//! ## 为何手卷而非 reqwest-middleware
//!
//! 2026 Rust HTTP 客户端业界 idiomatic 是 `reqwest-middleware` + `RetryableStrategy`
//! trait impl（composable / testable / scale）。SJTU-CLI 选手卷闭包 helper 的 4 条理由：
//!
//! 1. CLAUDE.md 不引入新依赖硬约束（middleware 需 +2 crate）
//! 2. 改造面 ×6 子系统（裸 reqwest::Client → ClientWithMiddleware sweeping refactor）
//! 3. Stateful side-effect（clear_sub_session + 重 CAS）在 stateless RetryableStrategy 里别扭
//! 4. 本轮 scope = 1 个子系统接入，1 处手卷更轻
//!
//! 未来若 retry 场景扩到 4+ 子系统，考虑迁 reqwest-middleware 重做（见
//! docs/superpowers/specs/2026-05-15-cas-retry-layer-design.md §1.3）。
//!
//! 同构 pattern 先例：src/commands/canvas_video/retry.rs::with_token_refresh（49 行
//! production 验证）。
```

---

## 9. 转 writing-plans 出口

本 spec 通过后调 `writing-plans` skill 出 task 级实装计划。预估 task 数：6-8（每 task 2-5 min step + TDD + commit gate）：

- **T1**：`SubSessionStale` variant 加 error.rs + 单测（强类型 + downcast 验证）
- **T2**：`Client::from_session` 拆分 + connect 改调 + LoginMeta Default + 测
- **T3**：`jwc/http.rs:142-148` detect 改抛 SubSessionStale + 行数 check
- **T4**：`auth/cas/retry.rs` with_cas_refresh + is_sub_session_stale + 3 单测（refresh 注入 vs cfg(test) 任选）
- **T5**：4 个 commands/jwc/handlers.rs handler 改 with_cas_refresh 闭包 + LOGIN_URL pub(crate) 暴露
- **T6**：gpa_handlers / schedule_handlers / ical/handler 余下 5 个 call site 改造
- **T7**：mockito 集成测（jwc_retry_integration.rs）+ BASE 注入钩子（OQ1 决策落地）
- **T8**：真机 smoke CP-CR-1 + CP-CR-2 + CP-CR-3 + README/SKILL/lessons.md 收尾

---

**Review checkpoints**（plan 阶段每 task 必跑）：
- `cargo check` / `cargo clippy --all-targets`
- `cargo fmt --check`
- 行数 `wc -l` 卡 200 / 300 限
- `cargo test --lib` 整体绿
- 新 commit 信息符合 §Git 规范
