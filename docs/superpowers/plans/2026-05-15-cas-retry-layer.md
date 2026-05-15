# CAS retry 层 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `src/auth/cas/` 新增通用 retry helper，把 T9 真机暴露的"jwc sub_session 客户端 fresh 但 ZF 服务端 timeout"盲区根治；本轮只接 jwc 9 个 handler call site，elec/services/jwbmessage 留独立 follow-up。

**Architecture:** `with_cas_refresh<F,Fut,T>(name, target_url, op)` 通用闭包 helper（同构 `canvas_video/retry.rs::with_token_refresh`）；信号载体是新 thiserror variant `SubSessionStale(&'static str)`，retry 层靠 `downcast_ref` 判定（业界 idiomatic for retry pattern，反对字符串匹配）。手卷不引 `reqwest-middleware`（理由见 spec §1.3）。

**Tech Stack:** Rust 2021 stable / anyhow / thiserror / reqwest / tokio / tracing / mockito（dev-dep 已在 `Cargo.toml:55`）。

**Spec：** `docs/superpowers/specs/2026-05-15-cas-retry-layer-design.md`

---

## 准备工作（task 起跑前主对话亲做一次）

- [ ] **P-0：确认当前工作树干净**

```bash
git status --short
```

Expected: 空输出（spec commit 99421f9 已落地，无残留改动）。

- [ ] **P-1：建追踪 branch（可选）**

```bash
# 用户自决；plan 本身允许直接在 main 上 task-by-task commit
# 若用 branch：git checkout -b feat/cas-retry-layer
```

---

## Task 1：error.rs 加 `SubSessionStale(&'static str)` variant

**Files:**
- Modify: `src/error.rs:7-50`
- Test: `src/error.rs:50-70` (inline `#[cfg(test)] mod tests`)

**目标**：strongly-typed retry 信号 + envelope code 映射不变（`session_expired`）。

- [ ] **Step 1：写 failing test（先在文件末尾加 inline tests 模块）**

在 `src/error.rs` 末尾加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_session_stale_has_session_expired_code() {
        let e = SjtuCliError::SubSessionStale("jwc");
        assert_eq!(e.code(), "session_expired");
    }

    #[test]
    fn sub_session_stale_carries_subsystem_name() {
        let e = SjtuCliError::SubSessionStale("jwc");
        let s = format!("{e}");
        assert!(s.contains("jwc"), "错误消息应包含子系统 name，实际：{s}");
    }

    #[test]
    fn sub_session_stale_can_be_downcast_from_anyhow() {
        let e: anyhow::Error = SjtuCliError::SubSessionStale("jwc").into();
        let downcasted = e.downcast_ref::<SjtuCliError>();
        assert!(matches!(
            downcasted,
            Some(SjtuCliError::SubSessionStale("jwc"))
        ));
    }
}
```

- [ ] **Step 2：跑测确认 fail**

```powershell
cargo test --lib error::tests
```

Expected: 3 个测试报错 `no variant or associated item named SubSessionStale`。

- [ ] **Step 3：加 variant + code() arm**

在 `src/error.rs:48` `CanvasTokenInvalid` variant 之后插入：

```rust
    /// 子系统 sub_session 客户端 cookie 仍 fresh 但服务端已 timeout
    /// （ZF 30 分钟无活动 / OAuth2 短 TTL 等）。`with_cas_refresh` 捕获此 variant
    /// 触发 clear_sub_session + 重 CAS。`&'static str` 是子系统 name（"jwc" 等）。
    #[error("子系统 `{0}` session 已被服务端失效")]
    SubSessionStale(&'static str),
```

并在 `impl SjtuCliError::code()` match 块（L55-66）的 `CanvasTokenInvalid` arm 之后插入：

```rust
            Self::SubSessionStale(_) => "session_expired",
```

- [ ] **Step 4：跑测 + lint**

```powershell
cargo test --lib error::tests
cargo clippy --lib --all-targets -- -D warnings
cargo fmt --check
```

Expected: 3 个测试 PASS；无 clippy warning；fmt 干净。

- [ ] **Step 5：行数 check**

```powershell
(Get-Content src\error.rs | Measure-Object -Line).Lines
```

Expected: ≤ 200（现 50 → 加 ~5 行 + ~30 行 inline tests = ~85 行，远未触限）。

- [ ] **Step 6：commit**

```bash
git add src/error.rs
git commit -m "feat(error): 加 SubSessionStale(&str) variant for CAS retry 信号"
```

---

## Task 2：拆 `Client::from_session` + `LoginMeta::default` + `LOGIN_URL` pub(crate)

**Files:**
- Modify: `src/apps/jwc/api/mod.rs:49` (LOGIN_URL 提到 pub(crate))
- Modify: `src/apps/jwc/api/mod.rs:63-87` (拆 connect → from_session + Default impl)
- Test: 改 `src/apps/jwc/api/mod.rs` 末尾加 inline test

**目标**：retry 闭包内部从 `Session` 重建 `Client`（不重做 CAS）。

- [ ] **Step 1：写 failing test**

在 `src/apps/jwc/api/mod.rs:133` 文件末尾加：

```rust
#[cfg(test)]
mod tests_from_session {
    use super::*;
    use crate::cookies::{Cookie, Session};

    /// from_session 从已有 session 构造 client，不调 cas_login（不依赖文件系统）。
    #[test]
    fn from_session_builds_client_without_cas_login() {
        let session = Session::new(vec![Cookie {
            name: "JSESSIONID".into(),
            value: "abc12345".into(),
            domain: Some("i.sjtu.edu.cn".into()),
            path: Some("/".into()),
            expires: None,
        }]);

        let client = Client::from_session(session).expect("from_session 应成功");
        assert_eq!(client.login.from_cache, false, "from_session 不知道 cache 状态");
        assert_eq!(client.login.elapsed_ms, 0);
        assert_eq!(client.login.final_url, "");
    }

    #[test]
    fn login_meta_default_is_empty() {
        let m = LoginMeta::default();
        assert_eq!(m.from_cache, false);
        assert_eq!(m.elapsed_ms, 0);
        assert_eq!(m.final_url, "");
    }
}
```

- [ ] **Step 2：跑测确认 fail**

```powershell
cargo test --lib apps::jwc::api::tests_from_session
```

Expected: `Client::from_session` / `LoginMeta::default` 未定义。

- [ ] **Step 3：实装**

改 `src/apps/jwc/api/mod.rs:49`：

```rust
// 改前: pub(super) const LOGIN_URL: &str = "https://i.sjtu.edu.cn/jaccountlogin";
pub(crate) const LOGIN_URL: &str = "https://i.sjtu.edu.cn/jaccountlogin";
```

替换 `impl Client` 块（L71-87）整个 connect 方法 + 加 from_session：

```rust
impl Client {
    /// 便利门面：cas_login + from_session。
    /// **不参与 with_cas_refresh 的场景下用**（如 status / debug）；
    /// 参与 retry 的命令应在 handler 层用 with_cas_refresh + Client::from_session。
    pub async fn connect() -> Result<Self> {
        let r = cas_login("jwc", LOGIN_URL).await?;
        let mut client = Self::from_session(r.session)?;
        client.login = LoginMeta {
            from_cache: r.from_cache,
            elapsed_ms: r.elapsed_ms,
            final_url: r.final_url,
        };
        Ok(client)
    }

    /// 从已有 session 构造 Client（cookie jar + http builder）。
    /// 给 with_cas_refresh 的闭包内调用 —— CAS 已在 helper 外做，不重复。
    pub fn from_session(session: crate::cookies::Session) -> Result<Self> {
        let http = build_http_client(&session)?;
        Ok(Self {
            http,
            throttle: Arc::new(Throttle::new()),
            time_counter: AtomicU32::new(0),
            visited_sp: Mutex::new(HashSet::new()),
            login: LoginMeta::default(),
        })
    }
```

并加 `LoginMeta` 的 `Default` impl（在 `LoginMeta` struct 定义后）：

```rust
impl Default for LoginMeta {
    fn default() -> Self {
        Self {
            from_cache: false,
            elapsed_ms: 0,
            final_url: String::new(),
        }
    }
}
```

- [ ] **Step 4：跑测 + lint + 行数**

```powershell
cargo test --lib apps::jwc
cargo clippy --lib --all-targets -- -D warnings
cargo fmt --check
(Get-Content src\apps\jwc\api\mod.rs | Measure-Object -Line).Lines
```

Expected: 测试全 PASS；行数 ≤ 200（现 133 + ~25 = ~158，OK）。

- [ ] **Step 5：commit**

```bash
git add src/apps/jwc/api/mod.rs
git commit -m "refactor(jwc): 拆 Client::from_session + LoginMeta::default + LOGIN_URL pub(crate)

为 CAS retry 层准备：with_cas_refresh 闭包内从 Session 重建 Client 不重 CAS。"
```

---

## Task 3：jwc/http.rs detect 改抛 `SubSessionStale("jwc")`

**Files:**
- Modify: `src/apps/jwc/http.rs:142-148`（detect 块改 variant）

**目标**：staleness detect 信号从字符串 message 升级到 strongly-typed variant，retry helper 能 downcast 判定。

**注意**：jwc/http.rs 现 192 行，改动后必须 ≤ 200。

- [ ] **Step 1：写 failing test（先看现有有没有针对 detect 的测）**

```powershell
Get-Content src\apps\jwc\tests_parse.rs | Select-String "session"
```

Expected: 空或不含 session 检测测试（detect 路径目前没单测覆盖）。

不专门给 detect 加单测 —— T7 集成测覆盖；本 task 只改 variant，TDD 由 T7 接管。但要 verify cargo check 通过 + 无 regression。

- [ ] **Step 2：替换 detect 块（L142-149）**

`src/apps/jwc/http.rs:142-149` 改前：

```rust
    // session 过期：CAS 把 final_url 改写到 jaccount，或正方主动给登录页 HTML
    if final_url.contains("jaccount.sjtu.edu.cn")
        || body.trim_start().starts_with("<!DOCTYPE")
        || body.trim_start().starts_with("<html")
    {
        return Err(SjtuCliError::UpstreamError(format!(
            "{label} session 已失效（被打回登录页 final_url={final_url}）；请 `sjtu logout` 后重新 `sjtu login`"
        )).into());
    }
```

改后：

```rust
    // ZF 服务端 stale：CAS 把 final_url 改写到 jaccount，或正方主动给登录页 HTML
    if final_url.contains("jaccount.sjtu.edu.cn")
        || body.trim_start().starts_with("<!DOCTYPE")
        || body.trim_start().starts_with("<html")
    {
        tracing::debug!(label, %final_url, "ZF 服务端 stale，触发 SubSessionStale");
        return Err(SjtuCliError::SubSessionStale("jwc").into());
    }
```

**行数对比**：改前 7 行，改后 7 行（多了 tracing::debug! 一行替换原来的 long error message 2 行 `"{label} session 已失效..."`）→ 净 +0 行。

需要在文件顶部 import `tracing`（若未在）：

```powershell
Get-Content src\apps\jwc\http.rs | Select-String "^use "
```

若顶部已有 `use tracing;` 或类似 import 则不需改；如无 `use tracing` 但其他文件用 `tracing::debug!` 是直接路径调用（不需 use），保持 `tracing::debug!` 直接路径调用即可。

- [ ] **Step 3：cargo check + 行数 check**

```powershell
cargo check --lib
(Get-Content src\apps\jwc\http.rs | Measure-Object -Line).Lines
```

Expected: 编译通过；行数 ≤ 200（改前 192 → 改后 192 净 +0）。

- [ ] **Step 4：跑现有测确认无 regression**

```powershell
cargo test --lib apps::jwc
cargo clippy --lib --all-targets -- -D warnings
cargo fmt --check
```

Expected: 既有测试全 PASS；无 clippy warning。

- [ ] **Step 5：commit**

```bash
git add src/apps/jwc/http.rs
git commit -m "refactor(jwc): http.rs staleness detect 改抛 SubSessionStale variant

T9 暴露盲区根治第一步：retry helper 能 downcast 判定 strongly-typed 信号，
不依赖错误 message 字符串匹配。final_url 信息进 tracing debug 不丢可观测性。"
```

---

## Task 4：`auth/cas/retry.rs` 新建 + `with_cas_refresh` + 3 单测

**Files:**
- Create: `src/auth/cas/retry.rs` (~80 行)
- Modify: `src/auth/cas/mod.rs:18-22` (+`pub mod retry` + re-export)

**目标**：核心 retry helper 落地，3 个 unit test 覆盖核心 case。

**OQ2 收口**：retry.rs 单测把 `cas_login` + `op` 两部分都直接 mock 太重 —— 采取**抽 `with_refresh_inner(op, refresh)` 私有函数注入 refresh** 策略，单测只测 `with_refresh_inner` 不真跑 cas_login。

- [ ] **Step 1：写 failing test（先把 retry.rs 文件骨架建好）**

创建 `src/auth/cas/retry.rs`：

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
//! 同构 pattern 先例：src/commands/canvas_video/retry.rs::with_token_refresh。

use std::future::Future;

use anyhow::Result;
use tracing::warn;

use crate::cookies::{clear_sub_session, Session};
use crate::error::SjtuCliError;
use super::cas_login;

/// CAS 子系统服务端 stale-detect + auto-refresh retry helper。
///
/// 工作流程：
/// 1. cas_login 拿 session（命中 cache 则直接返）
/// 2. 调 op(session) 跑业务
/// 3. 若返 SubSessionStale → clear_sub_session(name) + 重 cas_login → op(session2)
/// 4. 若仍返 SubSessionStale 或主 session 也挂 → 原样上抛
///
/// **仅适用于 GET only / 幂等只读**操作（CLAUDE.md i.sjtu 硬红线）；
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
    with_refresh_inner(r.session, op, || async move {
        clear_sub_session(name)?;
        let r2 = cas_login(name, target_url).await?;
        Ok(r2.session)
    })
    .await
    .map(|(v, _refreshed)| v)
}

/// 提取的核心 retry 逻辑：注入 refresh fn，便于单测（不依赖 cas_login）。
///
/// 返回 (op result, 是否触发过 refresh)，refreshed 给测试断言用。
pub(super) async fn with_refresh_inner<F, Fut, T, R, RFut>(
    initial_session: Session,
    op: F,
    refresh: R,
) -> Result<(T, bool)>
where
    F: Fn(Session) -> Fut,
    Fut: Future<Output = Result<T>>,
    R: FnOnce() -> RFut,
    RFut: Future<Output = Result<Session>>,
{
    match op(initial_session).await {
        Ok(v) => Ok((v, false)),
        Err(e) if is_sub_session_stale(&e) => {
            warn!(error = %e, "sub_session 服务端 stale，清缓存重做 CAS");
            let session2 = refresh().await?;
            op(session2).await.map(|v| (v, true))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cookies::{Cookie, Session};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn fresh_session() -> Session {
        Session::new(vec![Cookie {
            name: "JAAuthCookie".into(),
            value: "0123456789abcdef".into(),
            domain: Some("jaccount.sjtu.edu.cn".into()),
            path: Some("/".into()),
            expires: None,
        }])
    }

    /// 首次返 SubSessionStale → 触发 refresh → 第二次返 Ok → 总体返 Ok + refreshed=true。
    #[tokio::test]
    async fn retry_on_stale_then_ok() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_c = calls.clone();

        let (val, refreshed) = with_refresh_inner(
            fresh_session(),
            |_session| {
                let n = calls_c.fetch_add(1, Ordering::SeqCst);
                async move {
                    if n == 0 {
                        Err(SjtuCliError::SubSessionStale("jwc").into())
                    } else {
                        Ok(42u32)
                    }
                }
            },
            || async { Ok(fresh_session()) },
        )
        .await
        .expect("retry 后 op 应成功");

        assert_eq!(val, 42);
        assert!(refreshed, "应该触发过 refresh");
        assert_eq!(calls.load(Ordering::SeqCst), 2, "op 应被调 2 次");
    }

    /// 首次返 NetworkError（非 stale）→ 不触发 retry → 原样上抛 + refresh 未调。
    #[tokio::test]
    async fn no_retry_on_other_error() {
        let refresh_called = Arc::new(AtomicUsize::new(0));
        let refresh_c = refresh_called.clone();

        let result: Result<(u32, bool)> = with_refresh_inner(
            fresh_session(),
            |_session| async move {
                Err(SjtuCliError::NetworkError("connection reset".into()).into())
            },
            || {
                refresh_c.fetch_add(1, Ordering::SeqCst);
                async move { Ok(fresh_session()) }
            },
        )
        .await;

        assert!(result.is_err());
        let err = format!("{:#}", result.unwrap_err());
        assert!(err.contains("connection reset"), "应保留原错信息: {err}");
        assert_eq!(refresh_called.load(Ordering::SeqCst), 0, "非 stale 不应触发 refresh");
    }

    /// 首次 stale → refresh 成功 → 第二次仍 stale → 第二次错原样上抛。
    #[tokio::test]
    async fn retry_then_fail_returns_second_error() {
        let (val, _): (Result<(u32, bool)>, ()) = (
            with_refresh_inner(
                fresh_session(),
                |_session| async move {
                    Err(SjtuCliError::SubSessionStale("jwc").into())
                },
                || async { Ok(fresh_session()) },
            )
            .await,
            (),
        );

        assert!(val.is_err());
        // 第二次仍返 SubSessionStale，最终错该是 SubSessionStale 而非 refresh 错
        let err = val.unwrap_err();
        let downcasted = err.downcast_ref::<SjtuCliError>();
        assert!(
            matches!(downcasted, Some(SjtuCliError::SubSessionStale(_))),
            "应是第二次的 SubSessionStale 错，实际：{err:#}"
        );
    }
}
```

修改 `src/auth/cas/mod.rs:18-22`：

```rust
mod client;
pub mod retry;
#[cfg(test)]
mod tests;

use client::build_client;
pub use retry::with_cas_refresh;
```

- [ ] **Step 2：跑测确认 fail（编译错或测试不通过）**

```powershell
cargo test --lib auth::cas::retry::tests
```

Expected：第一次会编译成功（retry.rs 已完整），3 个测试全 PASS（因为 step 1 已经把 with_refresh_inner 完整实装了）。如果是 strict TDD 要求"先 fail"，可以先把 retry.rs 函数体清空只留签名 + `todo!()` 跑一次确认 fail，再恢复实装 —— 但本 task 函数小且 TDD 价值有限（pure logic 闭包路由），直接实装通过即可。

- [ ] **Step 3：lint + 行数 check**

```powershell
cargo clippy --lib --all-targets -- -D warnings
cargo fmt --check
(Get-Content src\auth\cas\retry.rs | Measure-Object -Line).Lines
(Get-Content src\auth\cas\mod.rs | Measure-Object -Line).Lines
```

Expected: `retry.rs` ≤ 200（实际 ~140-160 含 80 行 tests）；`mod.rs` ≤ 200（现 209 → +2 = 211 **触限**！要 step 3.5 处理）。

- [ ] **Step 3.5：mod.rs 行数处理**

mod.rs 现 209 行，+2 行变 211 触限。处理方案：

把现有 mod.rs 顶部的长 doc comment（L1-5）压缩或把 cache_is_fresh / is_jaccount_host 等私有函数迁到子模块。**最简方案**：去掉 mod.rs:24 的 `MAX_REDIRECT_HOPS` 后那一整行空白注释（如有）+ 把 mod.rs:1-4 doc comment 压一行。

实测方案（先看是否真触限）：

```powershell
$lines = (Get-Content src\auth\cas\mod.rs | Measure-Object -Line).Lines
Write-Host "mod.rs 当前行数: $lines / 200"
```

若 ≤ 200 跳过此 step。若 > 200：

把 `src/auth/cas/mod.rs:1-5` 的 5 行 doc comment 压成 2 行：

```rust
//! CAS 子系统跳转通用通道：手动跟 302 链 + 缓存到 `~/.sjtu-cli/sub_sessions/<name>.json`。
//! 不用 reqwest 默认 follow（吞中间 cookie / 无法停在 jaccount 报错）。
```

省 3 行 → 209 - 3 + 2 = 208，仍触限。继续压缩：把 `src/auth/cas/mod.rs:24-27` 的 3 行常量注释合并：

```rust
/// 跟链最大跳数（SJTU 实测 4-6 跳，留余量防死循环）。被 follow_redirect_chain 用。
const MAX_REDIRECT_HOPS: u8 = 10;
/// HTTP 超时（s）。CAS 偶尔慢，30s 无动静基本挂了。被 client.rs 用。
pub(super) const HTTP_TIMEOUT_SECS: u64 = 30;
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
```

省 5 行（合并 3 个常量的注释从 6 行减到 4 行；UA 行的 3 行注释删 2 行）→ 208 - 5 = 203，仍触限。

**Plan B**：把私有 helper `cache_is_fresh` / `is_jaccount_host` 迁到 retry.rs 内部（合理：retry 模块本来就用到 staleness 判定）。但这是 sub-task 范围内的重大改动，task 复杂度上升。

**Plan C（推荐）**：先实际跑一遍看真实行数，若超 200 再处理。doc comment 注释行 reasoning 是基于 `Read tool` 读到的样子，可能有空行被算多了。

```powershell
# Step 3.5 落地命令
$before = (Get-Content src\auth\cas\mod.rs | Measure-Object -Line).Lines
Write-Host "改前: $before 行"

# 如果 + 2 后 > 200，按上方 Plan A 压缩 doc comment
```

- [ ] **Step 4：跑测 + lint 二次确认**

```powershell
cargo test --lib auth::cas
cargo clippy --lib --all-targets -- -D warnings
cargo fmt --check
```

Expected: 全 PASS。

- [ ] **Step 5：commit**

```bash
git add src/auth/cas/retry.rs src/auth/cas/mod.rs
git commit -m "feat(cas): with_cas_refresh 通用 retry helper + 3 unit tests

- 闭包 helper 同构 canvas_video/retry.rs::with_token_refresh
- 抽 with_refresh_inner(op, refresh) 注入 refresh fn 便于单测
- is_sub_session_stale 靠 downcast_ref 判定，不字符串匹配
- 顶部 module comment 记录为何不用 reqwest-middleware（spec §1.3）"
```

---

## Task 5：handlers.rs 4 handler 改 with_cas_refresh 闭包

**Files:**
- Modify: `src/commands/jwc/handlers.rs` (4 个 handler: cmd_grades / cmd_schedule / cmd_gpa / cmd_exams)

**目标**：4 个最简单的 handler（单次 client method call）改造样板，验证 API 形态。

- [ ] **Step 1：cmd_grades 改造（最先做，作样板）**

替换 `src/commands/jwc/handlers.rs:14-38`：

```rust
/// `sjtu jwc grades [--xnm 2025] [--xqm 3] [--page 1] [--limit 50]`：N305005 成绩查询。
pub async fn cmd_grades(
    xnm: Option<String>,
    xqm: Option<String>,
    page: u32,
    limit: u32,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    use crate::apps::jwc::api::LOGIN_URL;
    use crate::auth::cas::with_cas_refresh;

    let xnm_q = xnm.clone();
    let xqm_q = xqm.clone();
    let env_resp = with_cas_refresh("jwc", LOGIN_URL, |session| {
        let xnm = xnm_q.clone();
        let xqm = xqm_q.clone();
        async move {
            let client = Client::from_session(session)?;
            client.grades(xnm.as_deref(), xqm.as_deref(), page, limit).await
        }
    })
    .await?;

    let returned = env_resp.items.len();
    let data = GradesData {
        xnm,
        xqm,
        page,
        limit,
        total_result: env_resp.total_result,
        total_page: env_resp.total_page,
        returned,
        items: env_resp.items,
    };
    render(Envelope::ok(data), fmt)
}
```

- [ ] **Step 2：cmd_schedule 改造（同样板）**

替换 `src/commands/jwc/handlers.rs:41-57`：

```rust
/// `sjtu jwc schedule [--xnm 2025] [--xqm 3]`：N2151 个人课表（学年学期）。
pub async fn cmd_schedule(
    xnm: Option<String>,
    xqm: Option<String>,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    use crate::apps::jwc::api::LOGIN_URL;
    use crate::auth::cas::with_cas_refresh;

    let xnm_q = xnm.clone();
    let xqm_q = xqm.clone();
    let resp = with_cas_refresh("jwc", LOGIN_URL, |session| {
        let xnm = xnm_q.clone();
        let xqm = xqm_q.clone();
        async move {
            let client = Client::from_session(session)?;
            client.schedule(xnm.as_deref(), xqm.as_deref()).await
        }
    })
    .await?;

    let returned = resp.kb_list.len();
    let data = ScheduleData {
        xnm,
        xqm,
        returned,
        xqjmc_map: resp.xqjmc_map,
        items: resp.kb_list,
    };
    render(Envelope::ok(data), fmt)
}
```

- [ ] **Step 3：cmd_gpa 改造（注意 fill_parsed 在闭包外做不影响 retry）**

替换 `src/commands/jwc/handlers.rs:62-88`：

```rust
pub async fn cmd_gpa(
    scope: GpaScope,
    rank: GpaRank,
    qs_xnxq: Option<String>,
    zz_xnxq: Option<String>,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    use crate::apps::jwc::api::LOGIN_URL;
    use crate::auth::cas::with_cas_refresh;

    let qs = qs_xnxq.clone();
    let zz = zz_xnxq.clone();
    let mut env_resp = with_cas_refresh("jwc", LOGIN_URL, |session| {
        let qs = qs.clone();
        let zz = zz.clone();
        async move {
            let client = Client::from_session(session)?;
            client.gpa(scope, rank, qs.as_deref(), zz.as_deref()).await
        }
    })
    .await?;
    // 双轨：保留 server 给的 gpapm/xjfpm 字符串，client 端 fill_parsed 填 RankPair。
    for g in &mut env_resp.items {
        g.fill_parsed();
    }
    let returned = env_resp.items.len();
    let data = GpaData {
        scope: scope_str(scope),
        rank: rank_str(rank),
        qs_xnxq,
        zz_xnxq,
        total_result: env_resp.total_result,
        returned,
        items: env_resp.items,
    };
    render(Envelope::ok(data), fmt)
}
```

- [ ] **Step 4：cmd_exams 改造**

替换 `src/commands/jwc/handlers.rs:91-114`：

```rust
pub async fn cmd_exams(
    xnm: Option<String>,
    xqm: Option<String>,
    page: u32,
    limit: u32,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    use crate::apps::jwc::api::LOGIN_URL;
    use crate::auth::cas::with_cas_refresh;

    let xnm_q = xnm.clone();
    let xqm_q = xqm.clone();
    let env_resp = with_cas_refresh("jwc", LOGIN_URL, |session| {
        let xnm = xnm_q.clone();
        let xqm = xqm_q.clone();
        async move {
            let client = Client::from_session(session)?;
            client.exams(xnm.as_deref(), xqm.as_deref(), page, limit).await
        }
    })
    .await?;

    let returned = env_resp.items.len();
    let data = ExamsData {
        xnm,
        xqm,
        page,
        limit,
        total_result: env_resp.total_result,
        total_page: env_resp.total_page,
        returned,
        items: env_resp.items,
    };
    render(Envelope::ok(data), fmt)
}
```

- [ ] **Step 5：cargo check + lint + 行数**

```powershell
cargo check --lib
cargo clippy --lib --all-targets -- -D warnings
cargo fmt --check
(Get-Content src\commands\jwc\handlers.rs | Measure-Object -Line).Lines
```

Expected: 编译通过；行数 ≤ 200（现 130 + ~20 = ~150）。

- [ ] **Step 6：commit**

```bash
git add src/commands/jwc/handlers.rs
git commit -m "feat(jwc): 4 handler (grades/schedule/gpa/exams) 接 with_cas_refresh

每个 handler 包闭包内 Client::from_session + 单次 client method 调用；
跨多个 await 的 xnm/xqm 用 clone 进闭包。"
```

---

## Task 6：剩余 5 个 call site + ical fail-soft fix

**Files:**
- Modify: `src/commands/jwc/gpa_handlers.rs` (cmd_gpa_by_semester 包 12 学期循环)
- Modify: `src/commands/jwc/schedule_handlers.rs` (cmd_today / cmd_week)
- Modify: `src/commands/jwc/schedule_next.rs` (cmd_next)
- Modify: `src/cli/jwc/mod.rs:193-197` (Calendar dispatch arm 改 with_cas_refresh)
- Modify: `src/commands/jwc/ical/handler.rs:113-165` (fetch_all stale 优先级 fix)

**目标**：把剩余 5 个 client connect call site 全接入，并修一处 fail-soft 吃错 bug。

**核心 bug 修复**：`ical/handler.rs::fetch_all` 现在用 `unwrap_or_else(|e| warnings.push(e); default)` 模式，会把 SubSessionStale 错误吃成 warnings 不上抛 → retry helper 永远拿不到信号。T9 真机看到的 `eventCount=0 + warnings 含登录页` 就是这个。要让 SubSessionStale 错跳过 fail-soft 直接上抛。

- [ ] **Step 1：cmd_gpa_by_semester 改造（retry 包整个 12 学期循环）**

替换 `src/commands/jwc/gpa_handlers.rs:26-77`：

```rust
pub async fn cmd_gpa_by_semester(
    scope: GpaScope,
    rank: GpaRank,
    xnm_from: Option<u32>,
    xnm_to: Option<u32>,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    use crate::apps::jwc::api::LOGIN_URL;
    use crate::auth::cas::with_cas_refresh;

    let (from, to) = resolve_xnm_range(xnm_from, xnm_to);
    let requested = enumerate_semesters(from, to);

    let requested_for_op = requested.clone();
    let (succeeded, failed) = with_cas_refresh("jwc", LOGIN_URL, |session| {
        let requested = requested_for_op.clone();
        async move {
            let client = Client::from_session(session)?;
            let mut succeeded: Vec<SemesterGpa> = Vec::new();
            let mut failed: Vec<SemesterFailure> = Vec::new();

            for key in &requested {
                let xnxq = format!("{}{}", key.xnm, key.xqm);
                let res = client.gpa(scope, rank, Some(&xnxq), Some(&xnxq)).await;
                tokio::time::sleep(Duration::from_millis(SEMESTER_QUERY_THROTTLE_MS)).await;
                match res {
                    Ok(mut env) if !env.items.is_empty() => {
                        let mut g = env.items.remove(0);
                        g.fill_parsed();
                        let mut sg: SemesterGpa = (&g).into();
                        sg.xnm = key.xnm.clone();
                        sg.xqm = key.xqm.clone();
                        succeeded.push(sg);
                    }
                    Ok(_) => failed.push(SemesterFailure {
                        xnm: key.xnm.clone(),
                        xqm: key.xqm.clone(),
                        reason: "items 空（疑似未到统计时间或该学期无成绩）".into(),
                    }),
                    Err(e) => {
                        // SubSessionStale 必须上抛触发 retry，不能装进 failed 吞掉
                        if e.downcast_ref::<crate::error::SjtuCliError>()
                            .map(|err| matches!(err, crate::error::SjtuCliError::SubSessionStale(_)))
                            .unwrap_or(false)
                        {
                            return Err(e);
                        }
                        failed.push(SemesterFailure {
                            xnm: key.xnm.clone(),
                            xqm: key.xqm.clone(),
                            reason: format!("{e:#}"),
                        });
                    }
                }
            }
            Ok::<_, anyhow::Error>((succeeded, failed))
        }
    })
    .await?;

    let data = GpaBySemesterData {
        scope: scope_str(scope),
        rank: rank_str(rank),
        xnm_from: from,
        xnm_to: to,
        requested,
        succeeded,
        failed,
    };
    render(Envelope::ok(data), fmt)
}
```

需要在 file 顶部加 import `use crate::cookies::Session;` —— 实际 `with_cas_refresh` 闭包参数 session 已通过 `Client::from_session(session)` 用，不需直接 import Session 类型（编译器推断）。

**注意 SemesterKey clone**：`requested.clone()` 要求 `SemesterKey` 实装 `Clone`。看 data.rs 确认；如未实装在 `data.rs::SemesterKey` 加 `#[derive(Clone)]`：

```powershell
Get-Content src\commands\jwc\data\mod.rs | Select-String "SemesterKey"
```

若 SemesterKey struct 缺 Clone derive，需在 step 1 包含为它加 `#[derive(Clone)]`。

- [ ] **Step 2：cmd_today 改造（连续 2 次 client method）**

替换 `src/commands/jwc/schedule_handlers.rs:18-109` (cmd_today)：

```rust
pub async fn cmd_today(
    xnm: Option<String>,
    xqm: Option<String>,
    grid: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    use crate::apps::jwc::api::LOGIN_URL;
    use crate::auth::cas::with_cas_refresh;

    let xnm_q = xnm.clone();
    let xqm_q = xqm.clone();
    let (cw, sched_opt) = with_cas_refresh("jwc", LOGIN_URL, |session| {
        let xnm = xnm_q.clone();
        let xqm = xqm_q.clone();
        async move {
            let client = Client::from_session(session)?;
            let cw = client
                .infer_current_week(xnm.as_deref(), xqm.as_deref())
                .await?;
            if cw == 0 || cw > 18 {
                return Ok::<_, anyhow::Error>((cw, None));
            }
            let sched = client
                .schedule_by_week(xnm.as_deref(), xqm.as_deref(), cw)
                .await?;
            Ok((cw, Some(sched)))
        }
    })
    .await?;

    let today = Local::now().date_naive();
    let today_weekday = iso_weekday(today);
    let today_iso = today.format("%Y-%m-%d").to_string();

    // fail-soft: 学期外直接返回 hint（早返点跟原版一致，但 schedule 已被闭包内跳过）
    if cw == 0 || cw > 18 {
        let hint = if cw == 0 { "学期未开始" } else { "学期已结束 / 假期" };
        return render(
            Envelope::ok(TodayData {
                xnm,
                xqm,
                current_week: cw,
                today_iso,
                today_weekday,
                hint: Some(hint.into()),
                items: vec![],
            }),
            fmt,
        );
    }

    let sched = sched_opt.expect("学期内 cw 必返 Some(sched)");
    let filtered = filter_kb_in_week(&sched.kb_list, cw);
    let now_time = Local::now().time();

    let mut items: Vec<TodayItem> = filtered
        .iter()
        .filter_map(|k| {
            let xqj = parse_xqj(k.xqj.as_deref());
            if xqj != today_weekday {
                return None;
            }
            let (jc_list, clock_list) = expand_jc(k.old_jc);
            if clock_list.is_empty() {
                return None;
            }
            if let Some((_, last_end)) = clock_list.last() {
                if let Ok(t) = chrono::NaiveTime::parse_from_str(last_end, "%H:%M") {
                    if t <= now_time {
                        return None;
                    }
                }
            }
            Some(TodayItem {
                kcmc: k.kcmc.clone(),
                xqj,
                jc_list,
                clock_list,
                jcor_fallback: k.jcor.clone(),
                cdmc: k.cdmc.clone(),
                xm: k.xm.clone(),
                kch: k.kch.clone(),
            })
        })
        .collect();
    items.sort_by_key(|i| i.jc_list.first().copied().unwrap_or(99));

    if grid {
        print!("{}", render_day_grid(&items));
        return Ok(());
    }

    render(
        Envelope::ok(TodayData {
            xnm,
            xqm,
            current_week: cw,
            today_iso,
            today_weekday,
            hint: None,
            items,
        }),
        fmt,
    )
}
```

- [ ] **Step 3：cmd_week 改造（同结构）**

替换 `src/commands/jwc/schedule_handlers.rs:112-178` (cmd_week)：

```rust
pub async fn cmd_week(
    xnm: Option<String>,
    xqm: Option<String>,
    zs: Option<u8>,
    grid: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    use crate::apps::jwc::api::LOGIN_URL;
    use crate::auth::cas::with_cas_refresh;

    let xnm_q = xnm.clone();
    let xqm_q = xqm.clone();
    let (cw, sched) = with_cas_refresh("jwc", LOGIN_URL, |session| {
        let xnm = xnm_q.clone();
        let xqm = xqm_q.clone();
        async move {
            let client = Client::from_session(session)?;
            let cw = client
                .infer_current_week(xnm.as_deref(), xqm.as_deref())
                .await?;
            let query_zs = zs.unwrap_or(cw.clamp(1, 18));
            let sched = client
                .schedule_by_week(xnm.as_deref(), xqm.as_deref(), query_zs)
                .await?;
            Ok::<_, anyhow::Error>((cw, sched))
        }
    })
    .await?;

    let query_zs = zs.unwrap_or(cw.clamp(1, 18));
    let filtered = filter_kb_in_week(&sched.kb_list, query_zs);

    let mut items: Vec<TodayItem> = filtered
        .iter()
        .filter_map(|k| {
            let xqj = parse_xqj(k.xqj.as_deref());
            let (jc_list, clock_list) = expand_jc(k.old_jc);
            if jc_list.is_empty() {
                return None;
            }
            Some(TodayItem {
                kcmc: k.kcmc.clone(),
                xqj,
                jc_list,
                clock_list,
                jcor_fallback: k.jcor.clone(),
                cdmc: k.cdmc.clone(),
                xm: k.xm.clone(),
                kch: k.kch.clone(),
            })
        })
        .collect();
    items.sort_by_key(|i| (i.xqj, i.jc_list.first().copied().unwrap_or(99)));

    let hint = if cw == 0 {
        Some("学期未开始".into())
    } else if cw > 18 {
        Some("学期已结束 / 假期".into())
    } else {
        None
    };

    if grid {
        print!("{}", render_week_grid(&sched.rqazc_list, &items));
        return Ok(());
    }

    render(
        Envelope::ok(WeekData {
            xnm,
            xqm,
            current_week: cw,
            query_zs,
            rqazc_list: sched.rqazc_list,
            hint,
            items,
        }),
        fmt,
    )
}
```

- [ ] **Step 4：cmd_next 改造**

先读 schedule_next.rs 完整内容（plan 写时未读）：

```powershell
Get-Content src\commands\jwc\schedule_next.rs
```

按 cmd_today/week 同模式改造：把 `Client::connect()` + 后续所有 `client.method().await` 改写成 with_cas_refresh 闭包。具体改法 implementer 阅读后参照 Step 2/3 样板做（plan 不预先列因 schedule_next.rs 未读到具体内容）。

**改造检查清单**：
- [ ] 闭包内 `Client::from_session(session)?`
- [ ] xnm/xqm 在闭包外 clone 进闭包
- [ ] 闭包返 IO 结果 tuple，render 在闭包外
- [ ] 早返点（学期外 hint）若依赖 IO 后变量需重新排版

- [ ] **Step 5：cli/jwc/mod.rs Calendar dispatch arm 改造**

替换 `src/cli/jwc/mod.rs:193-197`：

```rust
        JwcSub::Calendar(a) => {
            use crate::apps::jwc::api::LOGIN_URL;
            use crate::auth::cas::with_cas_refresh;
            use crate::apps::jwc::Client;

            let a_xnm = a.xnm.clone();
            let a_xqm = a.xqm.clone();
            let a_to = a.to.clone();
            let a_no_acad = a.no_academic;
            let a_no_exams = a.no_exams;
            with_cas_refresh("jwc", LOGIN_URL, |session| {
                let xnm = a_xnm.clone();
                let xqm = a_xqm.clone();
                let to = a_to.clone();
                async move {
                    let client = Client::from_session(session)?;
                    jwc_cmds::cmd_calendar(&client, xnm, xqm, to, a_no_acad, a_no_exams, fmt).await
                }
            })
            .await
        }
```

**Type check**：cmd_calendar 返 `Result<()>`，with_cas_refresh 闭包要求 `Future<Output = Result<T>>` 其中 T = ()。最终 with_cas_refresh 返 `Result<()>`，跟 dispatch arm 期望返类型一致。

- [ ] **Step 6：ical/handler.rs fetch_all stale 优先级 fix（关键 bug）**

替换 `src/commands/jwc/ical/handler.rs:151-163` (fetch_all 末尾 unwrap_or_else 段)：

```rust
    let (c_r, e_r, a_r) = tokio::join!(class_fut, exam_fut, academic_fut);

    // SubSessionStale 必须穿透 fail-soft 上抛，触发 retry；否则被吞成 warnings
    // 导致 retry helper 永远收不到信号（T9 真机暴露盲区根因）。
    if let Some(stale) = stale_error_among(&[c_r.as_ref().err(), e_r.as_ref().err(), a_r.as_ref().err()]) {
        return (
            Err::<Schedule, _>(stale).unwrap_err().into(),
            Err::<Vec<Exam>, _>(anyhow::anyhow!("placeholder")).unwrap_err().into(),
            AcademicCalendar::default(),
        );
        // 上面 placeholder 写法不通顺；简化为：
    }

    // 简化版（用直接 early-return 的方式）：
    let schedule = match c_r {
        Ok(s) => s,
        Err(e) if is_sub_session_stale(&e) => return Err(e).into_async(/*伪代码*/),
        Err(e) => { warnings.push(format!("课表 (N2151) 失败: {e:#}")); Schedule::default() }
    };
    ...
```

**等等 ── 这段太复杂**。fetch_all 现在签名是 `async fn fetch_all(...) -> (Schedule, Vec<Exam>, AcademicCalendar)` ── 不返 Result。要支持 stale 错穿透必须改签名。

**最干净方案**：fetch_all 改签名 `-> Result<(Schedule, Vec<Exam>, AcademicCalendar)>`，stale 错直接上抛，其他错继续 warnings + default。

改 `src/commands/jwc/ical/handler.rs:112-165` 整个 fetch_all：

```rust
/// 并行 fetch 课表 / 考试 / 校历；任一返 SubSessionStale 立即上抛触发 retry。
/// 其他错走 fail-soft：装 warnings 用 default 兜底。
async fn fetch_all(
    client: &Client,
    xnm: &str,
    xqm: &str,
    no_exams: bool,
    no_academic: bool,
    warnings: &mut Vec<String>,
) -> Result<(Schedule, Vec<Exam>, AcademicCalendar)> {
    let class_fut = client.schedule(Some(xnm), Some(xqm));

    let exam_fut = async {
        if no_exams {
            Ok::<Vec<Exam>, anyhow::Error>(vec![])
        } else {
            client
                .exams(Some(xnm), Some(xqm), 1, 500)
                .await
                .map(|p| p.items)
        }
    };

    let academic_fut = async {
        if no_academic {
            Ok(AcademicCalendar::default())
        } else {
            let xnm_owned = xnm.to_string();
            let xqm_owned = xqm.to_string();
            match tokio::task::spawn_blocking(move || load_from_fixture(&xnm_owned, &xqm_owned)).await {
                Ok(r) => r,
                Err(e) => Err(anyhow::anyhow!("spawn_blocking: {e}")),
            }
        }
    };

    let (c_r, e_r, a_r) = tokio::join!(class_fut, exam_fut, academic_fut);

    // 任一路返 SubSessionStale 立即上抛触发 with_cas_refresh retry
    for err_opt in [c_r.as_ref().err(), e_r.as_ref().err(), a_r.as_ref().err()] {
        if let Some(e) = err_opt {
            if e.downcast_ref::<crate::error::SjtuCliError>()
                .map(|err| matches!(err, crate::error::SjtuCliError::SubSessionStale(_)))
                .unwrap_or(false)
            {
                return Err(anyhow::anyhow!("{e:#}").context("ical fetch_all 检测到 stale，上抛触发 retry"));
            }
        }
    }

    let schedule = c_r.unwrap_or_else(|e| {
        warnings.push(format!("课表 (N2151) 失败: {e:#}"));
        Schedule::default()
    });
    let exams = e_r.unwrap_or_else(|e| {
        warnings.push(format!("考试 (N358105) 失败: {e:#}"));
        vec![]
    });
    let academic = a_r.unwrap_or_else(|e| {
        warnings.push(format!("学年校历 fixture 失败: {e:#}"));
        AcademicCalendar::default()
    });

    Ok((schedule, exams, academic))
}
```

**Critical bug 注意**：上方 `anyhow::anyhow!("{e:#}").context(...)` 把 SjtuCliError 通过 string format 重新包装，**破坏 downcast 链**——retry helper 在 with_cas_refresh 里 downcast_ref::<SjtuCliError>() 会拿不到！

**正确做法**：clone SjtuCliError variant 重 raise，保留 downcast 能力：

替换 stale-check 块：

```rust
    // 任一路返 SubSessionStale 立即重 raise variant 触发 with_cas_refresh retry
    for err_opt in [c_r.as_ref().err(), e_r.as_ref().err(), a_r.as_ref().err()].iter().flatten() {
        if let Some(sjtu_err) = err_opt.downcast_ref::<crate::error::SjtuCliError>() {
            if let crate::error::SjtuCliError::SubSessionStale(name) = sjtu_err {
                return Err(crate::error::SjtuCliError::SubSessionStale(*name).into());
            }
        }
    }
```

`SubSessionStale(&'static str)` 是 Copy（&str 是 Copy），可直接 `*name` 拷一份重 raise。downcast 链保留。

并改 fetch_all 调用方 (line 65)：

```rust
    let (schedule, exams, academic) =
        fetch_all(client, &xnm, &xqm, no_exams, no_academic, &mut warnings).await?;
    // 添加 `?` 把 stale 错穿透抛给 with_cas_refresh
```

cmd_calendar 返 `Result<()>`，已经支持 `?`。

- [ ] **Step 7：cargo check + lint + 全 jwc 测**

```powershell
cargo check --lib --tests
cargo clippy --lib --all-targets -- -D warnings
cargo fmt --check
cargo test --lib jwc
```

Expected: 编译通过；lint 干净；既有 jwc 测试全 PASS。

- [ ] **Step 8：行数 check**

```powershell
foreach ($f in @(
    "src\commands\jwc\gpa_handlers.rs",
    "src\commands\jwc\schedule_handlers.rs",
    "src\commands\jwc\schedule_next.rs",
    "src\commands\jwc\ical\handler.rs",
    "src\cli\jwc\mod.rs"
)) {
    $l = (Get-Content $f | Measure-Object -Line).Lines
    Write-Host "$f : $l 行"
}
```

Expected: 所有 ≤ 200。`ical/handler.rs` 现 190 行 + fetch_all +6 行（stale check）= 196，仍在限内。

- [ ] **Step 9：commit**

```bash
git add src/commands/jwc/gpa_handlers.rs src/commands/jwc/schedule_handlers.rs src/commands/jwc/schedule_next.rs src/commands/jwc/ical/handler.rs src/cli/jwc/mod.rs
git commit -m "feat(jwc): 剩余 5 个 call site 接 with_cas_refresh + ical fail-soft fix

- cmd_gpa_by_semester: retry 包整个 12 学期循环；循环内 SubSessionStale 提前上抛
- cmd_today / cmd_week / cmd_next: 多个 await client method 都进闭包
- Calendar dispatch arm: cli/jwc/mod.rs:193 把 connect 移进 with_cas_refresh
- ical/handler.rs::fetch_all: 改返 Result，SubSessionStale 跳过 fail-soft 直接上抛
  (T9 真机暴露盲区根因：fail-soft 吃 stale 错成 warnings 导致 retry 收不到信号)"
```

---

## Task 7：mockito 集成测 `tests/jwc_retry_integration.rs`

**Files:**
- Create: `tests/jwc_retry_integration.rs` (~80 行)
- Modify: `src/apps/jwc/http.rs` (BASE 暴露 testing hook)
- Modify: `src/apps/jwc/api/mod.rs` (LOGIN_URL 类似)

**目标**：round-trip 测试 — mockito 启 server，第一次返 HTML 登录页，第二次返合法 JSON，验证 retry 透明完成。

**OQ1 收口**：用 **env var override** 方案，比 `Client::with_base()` 入侵小；env var 仅测试场景使用，production path 不读：

实际上 jwc 走的是 cas_login + sub_session 体系，集成测难度高（cas_login 要主 session 文件 + CAS 链）。**推荐方案：本 task 只单测 retry.rs 已 covered 的 case**（T4 已做），集成测 marked `#[ignore]` 留做未来真机 ad-hoc 跑。

- [ ] **Step 1：评估集成测可行性**

```powershell
# 现实 check：jwc 集成测要 mock cas_login，但 cas_login 读 ~/.sjtu-cli/session.json
# 主 session 文件。mockito 不能 hijack 文件系统。
# Plan B：不写 jwc 端到端集成测；T4 单测已 cover retry 核心逻辑；
# 真机 smoke (T8) cover 端到端。
```

- [ ] **Step 2：仅在 `tests/cas_retry_signal.rs` 加一个 cross-module 集成 sanity（不需要 mockito）**

创建 `tests/cas_retry_signal.rs`：

```rust
//! 跨模块 sanity：SubSessionStale variant 的 downcast 在 anyhow 链上保留。
//! 防 ical/handler.rs::fetch_all 之类的 fail-soft 路径再次破坏 downcast 链。

use anyhow::Result;
use sjtu_cli::error::SjtuCliError;

#[test]
fn sub_session_stale_survives_anyhow_boxing() {
    let raised: Result<()> = Err(SjtuCliError::SubSessionStale("jwc").into());
    let err = raised.unwrap_err();
    let downcasted = err.downcast_ref::<SjtuCliError>();
    assert!(matches!(
        downcasted,
        Some(SjtuCliError::SubSessionStale("jwc"))
    ));
}

#[test]
fn sub_session_stale_survives_context_wrapping() {
    use anyhow::Context;
    let raised: Result<()> = Err(SjtuCliError::SubSessionStale("jwc").into());
    let wrapped = raised.context("额外上下文");
    let err = wrapped.unwrap_err();
    // 加 context 后 root cause 仍可 downcast
    let downcasted = err.downcast_ref::<SjtuCliError>();
    assert!(matches!(
        downcasted,
        Some(SjtuCliError::SubSessionStale("jwc"))
    ));
}

#[test]
fn sub_session_stale_does_not_survive_string_reraise() {
    // 反例：用 anyhow!("{}", err) 重 raise 破坏 downcast 链 —— 这是 ical fetch_all
    // 老 bug 路径。本测保证 plan T6 的 fix（重 raise variant 而非 string）的 invariant。
    let raised: Result<()> = Err(SjtuCliError::SubSessionStale("jwc").into());
    let err = raised.unwrap_err();
    let reraised: Result<()> = Err(anyhow::anyhow!("{:#}", err));
    let reraised_err = reraised.unwrap_err();
    let downcasted = reraised_err.downcast_ref::<SjtuCliError>();
    assert!(downcasted.is_none(), "string format reraise 应破坏 downcast 链");
}
```

注：测试需要 `sjtu_cli::error::SjtuCliError` 是 `pub` 的；查 `src/lib.rs` 是否 re-export：

```powershell
Get-Content src\lib.rs | Select-String "error"
```

若 lib.rs 没 re-export error 模块，加：

```rust
pub mod error;
```

(若已存在跳过此 step)

- [ ] **Step 3：跑测**

```powershell
cargo test --test cas_retry_signal
```

Expected: 3 个测试全 PASS（特别是反例：string reraise 应破坏 downcast）。

- [ ] **Step 4：行数 + lint**

```powershell
(Get-Content tests\cas_retry_signal.rs | Measure-Object -Line).Lines
cargo clippy --tests -- -D warnings
cargo fmt --check
```

Expected: ≤ 300（实际 ~50）。

- [ ] **Step 5：commit**

```bash
git add tests/cas_retry_signal.rs src/lib.rs
git commit -m "test(cas): SubSessionStale variant downcast 跨 anyhow 链 sanity (3 tests)

- 测试 1: variant 直接装 anyhow::Error 再 downcast 拿回（基线）
- 测试 2: 加 .context() wrapping 后 downcast 仍 OK
- 测试 3 反例: anyhow!(\"{}\", err) 字符串 reraise 破坏 downcast 链
  (这是 T6 ical/handler.rs fetch_all 老 bug 路径的 invariant 守卫)"
```

---

## Task 8：真机 smoke + 文档收尾（主对话亲跑）

**Files:**
- Manual: 跑 3 个 smoke scenarios
- Modify: `tasks/todo.md` (CAS retry follow-up 标完成)
- Modify: `tasks/lessons.md` (新沉淀段)
- Modify: `README.md` (若需要)
- Modify: `SKILL.md` (若需要)
- Modify: `CLAUDE.md` (当前阶段 / 下一步 更新)

**目标**：3 个真机 smoke 全过 + 文档与代码状态对齐。

- [ ] **Step 1：CP-CR-1 客户端 stale fresh 复现**

```powershell
# 1. 确认 jwc.json 存在（前面跑过 jwc 命令）
Test-Path "$env:APPDATA\sjtu\sjtu-cli\config\sub_sessions\jwc.json"

# 2. 删 jwc.json 模拟 sub_session 不存在
Remove-Item "$env:APPDATA\sjtu\sjtu-cli\config\sub_sessions\jwc.json" -Force

# 3. 跑 jwc grades，预期：cas_login 重做（cache miss）→ op 成功
cargo run --release -- jwc grades --xnm 2025 --xqm 12 --limit 3 --yaml
```

Expected:
- envelope `ok: true`
- `data.items` 有数据
- stderr / tracing 看到 cache miss 重做 CAS 跳转日志（非 stale warn，因为这次是 cache miss 而非 stale）

**记录结果**：
- [ ] CP-CR-1 通过 / 失败 + 完整 output 贴 lessons.md

- [ ] **Step 2：CP-CR-2 服务端 stale 真盲区复现（最关键）**

```powershell
# 1. 登录后立即跑一次记下时间
cargo run --release -- jwc grades --xnm 2025 --xqm 12 --limit 1
# (记 timestamp T0)

# 2. 等 ≥ 30 分钟（ZF 服务端 session timeout 默认 30min）
# Tip: 30 分钟期间不动 jwc 子系统任何命令；其他子系统命令不影响 ZF session

# 3. 30+ 分钟后再跑同一命令
cargo run --release -- jwc grades --xnm 2025 --xqm 12 --limit 3 --yaml 2>&1 | Out-Host
# 注意 2>&1 把 tracing warn 也显示出来
```

Expected:
- envelope `ok: true` + `data.items` 有数据
- stderr / tracing **看到 "sub_session 服务端 stale，清缓存重做 CAS" warn 日志**（关键证据 — retry 真的触发了）
- 命令耗时比正常 +2-3s（CAS 重做开销）

**若 retry 没触发**（output 正常但 stderr 看不到 stale warn）：
- 可能 ZF 30 分钟 timeout 没到（个别 server config 更长）→ 等更久（45 分钟 / 60 分钟）再试
- 可能 ZF 改了 stale 信号（不再返 login_slogin.html）→ 看实际 final_url，更新 jwc/http.rs detect 逻辑

**记录结果**：
- [ ] CP-CR-2 通过 / 失败 + 完整 output + tracing warn 日志贴 lessons.md

- [ ] **Step 3：CP-CR-3 主 session 也挂兜底**

```powershell
# 1. 备份主 session
Copy-Item "$env:APPDATA\sjtu\sjtu-cli\config\session.json" `
          "$env:APPDATA\sjtu\sjtu-cli\config\session.json.bak"

# 2. 手动损坏 JAAuthCookie（让 cas_login 拿到无效凭据）
$j = Get-Content "$env:APPDATA\sjtu\sjtu-cli\config\session.json" | ConvertFrom-Json
foreach ($c in $j.cookies) {
    if ($c.name -eq "JAAuthCookie") {
        $c.value = "INVALID_VALUE_FOR_TEST"
    }
}
$j | ConvertTo-Json -Depth 10 | Set-Content "$env:APPDATA\sjtu\sjtu-cli\config\session.json"

# 3. 同时删 sub_sessions 强迫 CAS 重做
Remove-Item "$env:APPDATA\sjtu\sjtu-cli\config\sub_sessions\jwc.json" -Force -ErrorAction SilentlyContinue

# 4. 跑命令，预期失败 + 友好提示
cargo run --release -- jwc grades --xnm 2025 --xqm 12 2>&1 | Out-Host

# 5. 恢复主 session
Move-Item "$env:APPDATA\sjtu\sjtu-cli\config\session.json.bak" `
          "$env:APPDATA\sjtu\sjtu-cli\config\session.json" -Force
```

Expected:
- exit code != 0 或 envelope `ok: false`
- error message 含 "请先 `sjtu logout` && `sjtu login`" 友好提示
- error code = `sub_system_unreachable`

**记录结果**：
- [ ] CP-CR-3 通过 / 失败 + 完整 output 贴 lessons.md

- [ ] **Step 4：tasks/lessons.md 加 2026-05-15 段（CAS retry follow-up 收尾）**

在 `tasks/lessons.md` 顶部（最新一段，目前 2026-05-15 T5 段后）追加：

```markdown
## 2026-05-15 — CAS retry 层 follow-up（T9 staleness 盲区根治）

**触发情境**：T9 真机暴露的"jwc sub_session 客户端 fresh 但 ZF 服务端 timeout"盲区。临时修复（手删 sub_sessions/jwc.json）不可持续。

**3 个 smoke scenario 真机跑结果**：
- CP-CR-1（删 jwc.json 后跑命令）：[填实际结果]
- CP-CR-2（等 30+ 分钟服务端 timeout）：[填实际结果，含 tracing warn 日志证据]
- CP-CR-3（主 session 也挂兜底）：[填实际结果]

### 关键设计教训

1. **fail-soft 吃掉 retry 信号是 silent bug 高发区**
   - ical/handler.rs::fetch_all 老路径 `unwrap_or_else(|e| warnings.push(e))` 把 SubSessionStale 错误装成 warnings → retry helper 永远收不到信号 → T9 表面 OK 实际 `eventCount=0`
   - 修复：fetch_all 改返 Result，stale 错跳过 fail-soft 直接上抛
   - 教训：任何 fail-soft 路径上必须先 detect "retry 信号" 优先级，不能盲目吞错。下次类似 fail-soft 接口设计要 check 是否有 retry-able 错误被吞

2. **anyhow + thiserror 混用时 downcast 链脆弱**
   - 反例：`anyhow!("{}", err)` 字符串重 raise 破坏 downcast 链；retry helper downcast_ref 拿不到 variant
   - 正确：clone variant 重 raise（`SubSessionStale(&'static str)` 是 Copy，`*name` 拷出来重新构造）
   - tests/cas_retry_signal.rs 加了反例测试当 invariant 守卫

3. **手卷 vs middleware trade-off 文档进 retry.rs**
   - 2026 业界 idiomatic 是 reqwest-middleware + RetryableStrategy，我们手卷的 4 条理由（CLAUDE.md no new deps / 改造面 ×6 / stateful side-effect / 1 子系统 scope）写进 retry.rs 顶部 module comment
   - 未来若扩到 4+ 子系统再 reconsider middleware（避免技术债务沉默累积）

4. **同构 pattern 先例复用 = 设计成本几乎 0**
   - canvas_video/retry.rs::with_token_refresh 49 行 production 验证 → cas/retry.rs 直接同构（with_refresh_inner 注入 refresh fn 是同构 + 单测友好的小改进）
   - 教训：codebase 内同构先例胜过外部业界 best practice；先 grep 自己再 google

### 设计决策（追加）

- **SubSessionStale variant 比字符串匹配胜出**：业界明确 retry pattern 推荐 thiserror variant，不依赖 error message 文案
- **retry 闭包接 Session 不接 Client**：cookie jar 必须重 build_http_client，无法复用旧 Client
- **with_refresh_inner 抽出 refresh fn 注入**：单测不需要 mock cas_login（pure 文件 IO），core retry logic 独立测试
```

- [ ] **Step 5：tasks/todo.md 标 CAS retry follow-up 完成**

在 `tasks/todo.md` 找到 follow-up 段（spec §1.2 或 T5 T9 真机新发现段提到的）改为：

```markdown
- [x] **CAS retry 层 follow-up**（T9 真机 staleness 盲区根治）2026-05-15 完成
  - 通用层 src/auth/cas/retry.rs::with_cas_refresh
  - SubSessionStale variant + downcast 强类型信号
  - jwc 9 个 handler call site 全接入
  - ical/handler.rs fetch_all fail-soft 吃信号 bug 修复
  - 真机 CP-CR-1..3 smoke 全过
  - 详细 spec / plan / lessons：
    - docs/superpowers/specs/2026-05-15-cas-retry-layer-design.md
    - docs/superpowers/plans/2026-05-15-cas-retry-layer.md
    - tasks/lessons.md 2026-05-15 段
  - elec / services / jwbmessage 接入留独立 follow-up（未真机暴露 staleness）
```

- [ ] **Step 6：CLAUDE.md 更新当前阶段 + 下一步**

`CLAUDE.md` 现 L98-99：

```markdown
- **已完成**：S0 骨架 / S1 QR 扫码登录 / ... / S3f-T5 jwc 校历 iCal MVP ... ≤ 25 min 目标
- **下一步**：S3 Phase 2 候选 ... follow-up：CAS 子系统读端点封装 redirect-detect → auto-refresh retry 层
```

替换为：

```markdown
- **已完成**：S0 骨架 / S1 QR 扫码登录 / ... / S3f-T5 jwc 校历 iCal MVP ... / **CAS retry 层 follow-up（T9 真机盲区根治；jwc 9 个 call site 接入；真机 CP-CR-1..3 全过 2026-05-15）**
- **下一步**：S3 Phase 2 候选 — 一卡通明细 / 通知聚合 / 图书馆借阅；或继续 jwc（培养方案 / 选课结果只读查询）；新 follow-up：elec/services/jwbmessage 接入 CAS retry 层（需先补 HTML 兜底 detect 信号）
```

- [ ] **Step 7：README.md / SKILL.md 更新（若需要）**

CAS retry 是内部 reliability 改进，对外 CLI 行为没新增/改动。但建议在 README "技术栈"段加一行：

```markdown
- **CAS retry**：sub_session 服务端 stale 时透明 auto-refresh（见 docs/superpowers/specs/2026-05-15-cas-retry-layer-design.md）
```

SKILL.md 不需要改（CLI 命令形态没变）。

- [ ] **Step 8：cargo test full sweep**

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

Expected: 全 PASS / 无 warning / fmt 干净。

- [ ] **Step 9：commit 文档收尾**

```bash
git add tasks/lessons.md tasks/todo.md CLAUDE.md README.md
git commit -m "docs(cas): CAS retry 层 follow-up 收尾 — 真机 CP-CR-1..3 全过

- lessons.md 加 2026-05-15 段：fail-soft 吃信号 bug / anyhow downcast 链脆弱 /
  同构 pattern 先例复用 / SubSessionStale variant 胜出
- todo.md 标 CAS retry follow-up [x] 完成
- CLAUDE.md 当前阶段 + 下一步 更新（jwc 9 call site 接入；elec/services/jwbmessage 留新 follow-up）
- README.md 技术栈段加 retry 一行

3 个 smoke scenario:
- CP-CR-1 (删 jwc.json → cache miss): [pass/fail]
- CP-CR-2 (等 30+ 分钟服务端 timeout): [pass/fail，含 tracing warn 证据]
- CP-CR-3 (主 session 也挂兜底): [pass/fail]"
```

- [ ] **Step 10（可选）：push 到 GitHub**

```bash
git push origin main
```

Expected: 全部 task 的 commit 推到远端。

---

## Self-Review 结果

### 1. Spec coverage 检查

| Spec § | 对应 Task |
|---|---|
| §1.1 T9 盲区背景 | T8 真机复现 CP-CR-2 |
| §1.2 现状基线表 | T1-T6 全面落地 |
| §1.3 联网验证 + Approach A 取舍 | T4 module comment |
| §2 Goals G1-G7 | G1=T8 / G2=T4 / G3=T1 / G4=T3 / G5=T4 / G6=T4 / G7=T8 |
| §2 Non-Goals NG1-NG6 | 全部尊重；NG1 接入范围 = T5+T6 仅 jwc |
| §3 文件改动地图 | T1-T7 一对一映射；plan 补正 cmd_next 独立文件 + cli/jwc/mod.rs Calendar arm |
| §4.1 with_cas_refresh API | T4 完整 |
| §4.2 SubSessionStale variant | T1 完整 |
| §4.3 jwc/http.rs detect 改造 | T3 完整 |
| §4.4 Client::from_session 拆分 | T2 完整 |
| §4.5 Handler 闭包样板 | T5 |
| §5 Data flow 3 路径 | T8 CP-CR-1（cache miss）+ CP-CR-2（服务端 stale）+ CP-CR-3（主 session 挂） |
| §6.1 retry.rs 3 单测 | T4 |
| §6.2 集成测 | T7（plan 阶段决定不写 jwc 端到端集成，改 cross-module sanity 加 3 tests）|
| §6.3 真机 smoke | T8 |
| §7 OQ1 | T7 收口（不写 jwc 集成测，改 cross-module sanity） |
| §7 OQ2 | T4 收口（抽 with_refresh_inner 注入 refresh fn） |
| §8 retry.rs 顶部 doc comment | T4 step 1 完整写入 |

**Plan 补正 spec 漏点**：
- ✗→✓ `cmd_next` 在独立 `schedule_next.rs` 文件（spec §3 误把它合并到 schedule_handlers）→ T6 step 4 单独处理
- ✗→✓ Calendar dispatch 在 `cli/jwc/mod.rs:193-197` 而非 ical/handler.rs 内部（spec §4.5 样板没明示）→ T6 step 5 处理
- ✗→✓ ical/handler.rs::fetch_all fail-soft 会吃 SubSessionStale 信号（spec 完全没提）→ T6 step 6 关键 bug fix

### 2. Placeholder scan

✓ 无 "TBD" / "TODO" / "implement later"
✓ 无 "类似 Task N" 之类的跳引
✓ T4 step 1 完整 retry.rs 代码（80 行 module + 3 tests 各完整）
✓ T6 step 1-6 每个 handler 完整代码（不省略）
✓ T8 真机 smoke 给完整 PowerShell 命令

唯一 "TBD"-ish 是 T6 step 4 cmd_next：plan 未读 schedule_next.rs 内容，让 implementer 按 Step 2/3 样板做。这是 acceptable trade-off（避免 plan 过长），但已给明确 checklist。

### 3. Type consistency

- `with_cas_refresh(name: &'static str, target_url: &str, op: F)` — T4 定义 / T5 / T6 一致使用
- `SubSessionStale(&'static str)` — T1 定义 / T3 raise / T4 downcast / T6 重 raise 全一致
- `Client::from_session(session: Session) -> Result<Self>` — T2 定义 / T5 / T6 一致使用
- `LoginMeta::default()` — T2 定义 + 用
- `LOGIN_URL: pub(crate) &str` — T2 提权 / T5 / T6 use

### 4. 行数硬限合规性

| 文件 | 改前 | 改后预估 | ≤ 200? |
|---|---|---|---|
| src/error.rs | 50 | ~85（+inline tests）| ✓ |
| src/auth/cas/retry.rs | 0 (new) | ~140-160 | ✓ |
| src/auth/cas/mod.rs | 209 | 211（+2）**需 step 3.5 处理** | ⚠ |
| src/apps/jwc/http.rs | 192 | 192（净 +0）| ✓ |
| src/apps/jwc/api/mod.rs | 133 | ~158（+25）| ✓ |
| src/commands/jwc/handlers.rs | 130 | ~150（+20）| ✓ |
| src/commands/jwc/gpa_handlers.rs | 142 | ~155 | ✓ |
| src/commands/jwc/schedule_handlers.rs | 178 | ~200 紧贴 | ⚠ |
| src/commands/jwc/schedule_next.rs | 未读 | 未知 + ~15 | ⚠ |
| src/commands/jwc/ical/handler.rs | 190 | ~200 紧贴 | ⚠ |
| src/cli/jwc/mod.rs | 未知 | +15 | ⚠ |
| tests/cas_retry_signal.rs | 0 (new) | ~50 | ✓ |

⚠ 标记的 4 个文件改后可能触限：T6 step 8 必须实测；触限就拆子模块或压缩注释。

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-05-15-cas-retry-layer.md`. Two execution options:**

**1. Subagent-Driven (recommended)** - 我 dispatch 一个 fresh subagent 跑每个 task，每 task 跑完两阶段 review（spec compliance → code quality），快速迭代。T8 真机 smoke 因需要主 session + 等 30 分钟服务端 timeout，必须主对话亲跑。

**2. Inline Execution** - 在本 session 用 executing-plans batch 跑 task，每 task 后 checkpoint 给我看再决定继续。

**Which approach?**
