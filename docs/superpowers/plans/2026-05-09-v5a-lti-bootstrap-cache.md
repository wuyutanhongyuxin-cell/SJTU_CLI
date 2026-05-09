# V5.A LTI Bootstrap 缓存 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 Canvas 课堂视频的 LTI launch（cas_login + headless Chrome + exchange_token）结果缓存到 sub_session 文件，30min TTL，把每条命令的 21-30s 启动开销在缓存命中时降到 < 1s；同时给 handlers 层加 token 失败回退（业务失败 / 401 / 403 自动清缓存重 launch 一次），并暴露 `clear-cache` 子命令。V5.B（18 讲 audio-only smoke）/ V5.C（转录调研）由本计划完成且 commit 后才触发。

**Architecture:** 在 `apps/canvas_video/` 下新增 `cache.rs`，对外只暴露 `lti_launch_cached` 与 `clear`；序列化结构 `CachedBootstrap` 含 `course_id` / `lti_tool_id` / `saved_at` / `ttl_secs` 自描述，原子写 + 反序列化容错让坏缓存自愈。`api.rs::Client::connect` 改一行从 `auth::lti_launch` 切到 `cache::lti_launch_cached`，对外契约不变。`handlers.rs` 新增 `with_token_refresh` 共享 helper，list / 单讲 download 套上做失败 1 次重试；批量下载不套（V4 fail-soft + 复跑幂等已是更优恢复路径）。最后挂 `clear-cache` CLI 子命令。

**Tech Stack:** Rust stable / chrono `DateTime<Utc>` / serde + serde_json / std::fs（原子 tmp+rename + chmod 600）/ anyhow + thiserror / tracing。无新依赖。

---

## 总览

V5.A spec：`docs/superpowers/specs/2026-05-09-v5-design.md`（commit `5614aea`）。本 plan 严格依 spec V5.A 章节实装；V5.B / V5.C 落在后续 plan，不在此覆盖。

### File Structure

涉及文件（最终行数预算见 spec line 197-209，全部 ≤ 200 限）：

| 文件 | 动作 | 增量 |
|---|---|---|
| `src/cookies/mod.rs` | 修改 | +1（`pub use io::sub_session_path`） |
| `src/apps/canvas_video/cache.rs` | 新建 | ~150 行 |
| `src/apps/canvas_video/mod.rs` | 修改 | +1（`mod cache;`） |
| `src/apps/canvas_video/api.rs` | 修改 | +0/-0（仅一行 import + 一行调用替换） |
| `src/apps/canvas_video/tests_parse.rs` | 修改 | +30（3 个新单测） |
| `src/commands/canvas_video/handlers.rs` | 修改 | +25（with_token_refresh + looks_like_token_invalid + cmd_clear_cache） |
| `src/commands/canvas_video/download_handler.rs` | 修改 | +3（cmd_download / cmd_download_all 改用 with_token_refresh） |
| `src/commands/canvas_video/data.rs` | 修改 | +6（ClearCacheData struct） |
| `src/commands/canvas_video/mod.rs` | 修改 | +1（pub use cmd_clear_cache） |
| `src/cli/canvas_video.rs` | 修改 | +6（ClearCache variant + dispatch arm） |
| `tasks/todo.md` | 修改 | +几行 V5.A 段 |

新文件 `cache.rs` 估计 ~150 行（spec 给的 ~100 行偏紧；多出来的是 chmod_600 + load_from_path/save_to_path 的可测拆分）。仍远低于 200 行限。

### TDD 范围说明

cache.rs 的纯逻辑（save / load / TTL / corruption）走 TDD（写测 → fail → 实装 → pass → commit）。
真实 LTI launch（spawn_blocking + Chrome）和 with_token_refresh 的回退路径**不走单测**：前者单测要 mock Chrome 不现实，后者 mock Client::connect 要引入 trait 抽象，溢出 V5.A 范围。这两块由真机 4 关收口（spec 第 187-194 行）。

---

## Task 1: 提升 `cookies::sub_session_path` 至 `pub`

cache.rs 要复用 cookies 模块已有的"子系统 session 路径 + 路径注入防御"（io.rs:44 的 `pub(super) fn sub_session_path`）。当前是 `pub(super)`，外部模块够不到。

**Files:**
- Modify: `src/cookies/mod.rs:16-19`

- [ ] **Step 1: 改 cookies/mod.rs 的 re-export**

把 `pub use io::{ ... }` 块加上 `sub_session_path`：

```rust
// src/cookies/mod.rs:16-19（修改）
pub use io::{
    clear_session, clear_sub_session, load_session, load_sub_session, save_session,
    save_sub_session, sub_session_path,
};
```

- [ ] **Step 2: 改 cookies/io.rs 的可见性**

把 `pub(super)` 改成 `pub`：

```rust
// src/cookies/io.rs:44（修改）
pub fn sub_session_path(name: &str) -> Result<std::path::PathBuf> {
```

- [ ] **Step 3: cargo check 验证**

Run: `cargo check --lib`
Expected: PASS（仅可见性放宽，无新调用点）

- [ ] **Step 4: Commit**

```powershell
git add src/cookies/mod.rs src/cookies/io.rs
git commit -m "refactor(cookies): 把 sub_session_path 提升至 pub 供 canvas_video::cache 复用"
```

---

## Task 2: cache.rs 骨架 — 模块声明 + CachedBootstrap 结构 + 转换 impl

新建 cache.rs，定义 CachedBootstrap 序列化结构 + `from_bootstrap` / `into_bootstrap` / `is_expired` impl。先不实装持久化，下一任务才 TDD save/load。

**Files:**
- Create: `src/apps/canvas_video/cache.rs`
- Modify: `src/apps/canvas_video/mod.rs:12-23`

- [ ] **Step 1: 在 canvas_video/mod.rs 注册 cache 子模块**

```rust
// src/apps/canvas_video/mod.rs:12-23（修改）
mod api;
mod api_form;
pub mod auth;
mod auth_chrome;
mod cache;
pub mod download;
pub mod ffmpeg;
mod http;
mod models;
mod models_video;
#[cfg(test)]
mod tests_parse;
mod throttle;
```

`mod cache;` 不加 `pub`：cache 仅供同 crate 内 api.rs / handlers.rs 调用，不做 lib 公开。

- [ ] **Step 2: 创建 cache.rs 骨架**

```rust
// src/apps/canvas_video/cache.rs（新建）
//! LTI launch Bootstrap 30min 缓存（V5.A）。
//!
//! 文件：`~/.sjtu-cli/sub_sessions/canvas_video_bootstrap_<course_id>_<lti_tool_id>.json`
//! 权限：Unix 600 / Windows ACL TODO（沿用 cookies/io.rs 同款 chmod_600 cfg-gate）。
//!
//! 失效路径：
//! - TTL 30min 过期 → load 返 None → 上层重 launch
//! - course_id / lti_tool_id 不匹配 → 防错配 → load 返 None
//! - JSON 反序列化失败 → 坏缓存 → load 返 None（下次 save 自愈）
//! - 业务失败（30min 内 token 提前作废）→ handlers 层 with_token_refresh 调 clear() 然后重 launch

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::models::Bootstrap;
use crate::cookies::{sub_session_path, Cookie};

/// 30 分钟。远低于 token 真实 TTL（1-3h），保守。
const TTL_SECS: u64 = 1800;

/// 文件命名前缀，clear() 用前缀匹配。
const FILE_PREFIX: &str = "canvas_video_bootstrap_";

/// 序列化到 sub_session 文件的结构（自描述：含 course_id / lti_tool_id / saved_at / ttl）。
/// 跟 Bootstrap 平铺等价 + 4 个元数据字段。
#[derive(Debug, Serialize, Deserialize)]
struct CachedBootstrap {
    course_id: u64,
    lti_tool_id: u64,
    saved_at: DateTime<Utc>,
    ttl_secs: u64,
    token: String,
    cour_id: String,
    lti_course_id: String,
    session_cookies: Vec<Cookie>,
}

impl CachedBootstrap {
    fn from_bootstrap(course_id: u64, lti_tool_id: u64, b: &Bootstrap) -> Self {
        Self {
            course_id,
            lti_tool_id,
            saved_at: Utc::now(),
            ttl_secs: TTL_SECS,
            token: b.token.clone(),
            cour_id: b.cour_id.clone(),
            lti_course_id: b.lti_course_id.clone(),
            session_cookies: b.session_cookies.clone(),
        }
    }

    fn into_bootstrap(self) -> Bootstrap {
        Bootstrap {
            token: self.token,
            cour_id: self.cour_id,
            lti_course_id: self.lti_course_id,
            session_cookies: self.session_cookies,
        }
    }

    /// `saved_at + ttl <= now` 视为过期。`now` 显式入参便于单测。
    fn is_expired(&self, now: DateTime<Utc>) -> bool {
        let ttl = Duration::seconds(self.ttl_secs as i64);
        self.saved_at + ttl <= now
    }
}

/// 缓存文件路径：`canvas_video_bootstrap_<course_id>_<lti_tool_id>.json`。
fn bootstrap_cache_path(course_id: u64, lti_tool_id: u64) -> Result<PathBuf> {
    sub_session_path(&format!("{FILE_PREFIX}{course_id}_{lti_tool_id}"))
}
```

- [ ] **Step 3: cargo check 验证**

Run: `cargo check --lib`
Expected: PASS（仅声明 + 结构 + impl，无未引用 warning，因为 `is_expired` / `bootstrap_cache_path` 等下一步 task 会被引用，会暂时 dead_code warn — 这步先打开 `#[allow(dead_code)]` 在文件顶或忽略 warn，**不要**用 `#![allow(...)]`：在每个未用 fn 上加 `#[allow(dead_code)]` 即可，等下一任务接入后再去掉）

实际操作：在本任务先不加 allow，**承认 cargo check 会报 dead_code warn 但 PASS**；等 Task 3 把 save/load 接进来后 warn 自动消。如果 CI 把 warn 当 error，那就在本任务加 `#[allow(dead_code)]` 在 `is_expired` / `bootstrap_cache_path` 上，Task 3 删掉。

- [ ] **Step 4: Commit**

```powershell
git add src/apps/canvas_video/cache.rs src/apps/canvas_video/mod.rs
git commit -m "feat(canvas_video): cache.rs 骨架 + CachedBootstrap 结构（V5.A 步 1）"
```

---

## Task 3: TDD `cache_round_trip` — `save_to_path` / `load_from_path` 完整逻辑

**Files:**
- Modify: `src/apps/canvas_video/cache.rs`（加 save_to_path / load_from_path / chmod_600）
- Modify: `src/apps/canvas_video/tests_parse.rs`（加 cache_round_trip 单测）

- [ ] **Step 1: 写失败测试 cache_round_trip**

在 `tests_parse.rs` 文件**末尾**追加：

```rust
// src/apps/canvas_video/tests_parse.rs（追加）

use super::cache;
use super::models::Bootstrap;
use crate::cookies::Cookie;

/// V5.A：CachedBootstrap 序列化 round-trip 完整保留所有字段。
#[test]
fn cache_round_trip() {
    let path = std::env::temp_dir().join("sjtu_cli_v5a_round_trip.json");
    let _ = std::fs::remove_file(&path);

    let bootstrap = Bootstrap {
        token: "eyJhbGciOiJIUzUxMiJ9.STUB.SIG".to_string(),
        cour_id: "abc/+def==".to_string(),
        lti_course_id: "0123456789abcdef0123456789abcdef".to_string(),
        session_cookies: vec![Cookie {
            name: "JSESSIONID".to_string(),
            value: "VALUE_X".to_string(),
            domain: Some("v.sjtu.edu.cn".to_string()),
            path: Some("/".to_string()),
            expires: None,
        }],
    };

    cache::save_to_path(&path, 88168, 8329, &bootstrap).unwrap();
    let loaded = cache::load_from_path(&path, 88168, 8329).expect("应能加载并通过校验");

    assert_eq!(loaded.token, bootstrap.token);
    assert_eq!(loaded.cour_id, bootstrap.cour_id);
    assert_eq!(loaded.lti_course_id, bootstrap.lti_course_id);
    assert_eq!(loaded.session_cookies.len(), 1);
    assert_eq!(loaded.session_cookies[0].name, "JSESSIONID");
    assert_eq!(loaded.session_cookies[0].value, "VALUE_X");

    // course_id 错配 → load 返 None
    assert!(cache::load_from_path(&path, 99999, 8329).is_none());
    // lti_tool_id 错配 → load 返 None
    assert!(cache::load_from_path(&path, 88168, 9999).is_none());

    let _ = std::fs::remove_file(&path);
}
```

- [ ] **Step 2: 跑测试确认 fail**

Run: `cargo test --lib -p sjtu-cli cache_round_trip -- --nocapture`
Expected: 编译失败（`cache::save_to_path` / `cache::load_from_path` 不存在）

- [ ] **Step 3: 实装 save_to_path / load_from_path / chmod_600**

cache.rs 追加（保持文件 < 200 行）：

```rust
// src/apps/canvas_video/cache.rs（追加，加在 bootstrap_cache_path 之后）

/// 保存 CachedBootstrap 到指定路径：原子写（tmp + rename）+ Unix chmod 600。
/// `pub(super)` 给 tests_parse.rs 用；prod 走 save() 包装。
pub(super) fn save_to_path(
    path: &Path,
    course_id: u64,
    lti_tool_id: u64,
    b: &Bootstrap,
) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("缓存路径无父目录: {}", path.display()))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("mkdir {} 失败", parent.display()))?;
    let cached = CachedBootstrap::from_bootstrap(course_id, lti_tool_id, b);
    let json = serde_json::to_string_pretty(&cached).context("序列化 CachedBootstrap 失败")?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json).with_context(|| format!("写 {} 失败", tmp.display()))?;
    chmod_600(&tmp)?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("rename {} → {} 失败", tmp.display(), path.display()))?;
    Ok(())
}

/// 从指定路径加载并通过 4 重校验：IO / JSON / id 匹配 / TTL。
/// 任一失败返 None（不 panic、不 propagate），上层调缓存命中失败重 launch 即可。
/// `pub(super)` 给 tests_parse.rs 用；prod 走 load() 包装。
pub(super) fn load_from_path(
    path: &Path,
    course_id: u64,
    lti_tool_id: u64,
) -> Option<Bootstrap> {
    let raw = std::fs::read_to_string(path).ok()?;
    let cached: CachedBootstrap = serde_json::from_str(&raw).ok()?;
    if cached.course_id != course_id || cached.lti_tool_id != lti_tool_id {
        debug!(
            file_course = cached.course_id,
            want_course = course_id,
            "Bootstrap 缓存 course_id / lti_tool_id 不匹配，忽略"
        );
        return None;
    }
    if cached.is_expired(Utc::now()) {
        debug!(course_id, lti_tool_id, "Bootstrap 缓存过期，忽略");
        return None;
    }
    Some(cached.into_bootstrap())
}

/// Unix 收 600 权限（cookies/io.rs 同款）。Windows 暂留 ACL TODO。
#[cfg(unix)]
fn chmod_600(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(path)?.permissions();
    perm.set_mode(0o600);
    std::fs::set_permissions(path, perm)
        .with_context(|| format!("无法设置 {} 权限为 600", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn chmod_600(_path: &Path) -> Result<()> {
    Ok(())
}
```

注意：本任务**不**实装 `save()` / `load()`（无路径参数的 prod 包装）。那两个在 Task 6 一起加。Task 4-5 只测 save_to_path / load_from_path。

- [ ] **Step 4: 跑测试确认 pass**

Run: `cargo test --lib -p sjtu-cli cache_round_trip -- --nocapture`
Expected: PASS

跑全量 lib 测一遍兜底：
Run: `cargo test --lib -p sjtu-cli`
Expected: 既有测试不挂 + cache_round_trip pass

- [ ] **Step 5: Commit**

```powershell
git add src/apps/canvas_video/cache.rs src/apps/canvas_video/tests_parse.rs
git commit -m "feat(canvas_video): cache.rs save_to_path/load_from_path + round_trip 单测（V5.A 步 2）"
```

---

## Task 4: TDD `cache_expired_returns_none` — TTL 校验

`load_from_path` 已经在 Task 3 实装了 TTL 检查；这一步是补**单独测它的 TTL 路径**（write 一个手填 stale saved_at 的文件，load 该返 None）。这是 spec 单测 #2。

**Files:**
- Modify: `src/apps/canvas_video/tests_parse.rs`

- [ ] **Step 1: 写失败测试 cache_expired_returns_none**

在 tests_parse.rs 末尾追加：

```rust
/// V5.A：cache_expired_returns_none — saved_at 早于 TTL 之外，load 返 None 不 panic。
#[test]
fn cache_expired_returns_none() {
    let path = std::env::temp_dir().join("sjtu_cli_v5a_expired.json");
    let _ = std::fs::remove_file(&path);

    // 手构造一个 31 分钟前 saved 的缓存（TTL 30min），写到磁盘。
    let stale = serde_json::json!({
        "course_id": 88168u64,
        "lti_tool_id": 8329u64,
        // 31 分钟前
        "saved_at": (chrono::Utc::now() - chrono::Duration::seconds(31 * 60)).to_rfc3339(),
        "ttl_secs": 1800u64,
        "token": "STALE",
        "cour_id": "X",
        "lti_course_id": "Y",
        "session_cookies": [],
    });
    std::fs::write(&path, serde_json::to_string_pretty(&stale).unwrap()).unwrap();

    let loaded = cache::load_from_path(&path, 88168, 8329);
    assert!(loaded.is_none(), "TTL 过期应返 None，但拿到了 {:?}", loaded.is_some());

    let _ = std::fs::remove_file(&path);
}
```

- [ ] **Step 2: 跑测试确认 pass**

因为 Task 3 已经把 TTL 检查写进了 load_from_path，这测应**直接 pass 不需新代码**。这是验证既有实装的覆盖。

Run: `cargo test --lib -p sjtu-cli cache_expired_returns_none -- --nocapture`
Expected: PASS（不修代码）

如果 fail：去 Task 3 步 3 看 `if cached.is_expired(Utc::now())` 那段，确认逻辑正确。

- [ ] **Step 3: Commit**

```powershell
git add src/apps/canvas_video/tests_parse.rs
git commit -m "test(canvas_video): cache_expired_returns_none 单测（V5.A 步 3）"
```

---

## Task 5: TDD `cache_corrupted_returns_none` — 反序列化失败兜底

spec 单测 #3。

**Files:**
- Modify: `src/apps/canvas_video/tests_parse.rs`

- [ ] **Step 1: 写测试 cache_corrupted_returns_none**

在 tests_parse.rs 末尾追加：

```rust
/// V5.A：cache_corrupted_returns_none — 垃圾 JSON / IO 失败均不 panic，load 返 None。
#[test]
fn cache_corrupted_returns_none() {
    let path = std::env::temp_dir().join("sjtu_cli_v5a_corrupted.json");
    let _ = std::fs::remove_file(&path);

    // 1) 文件不存在 → None
    assert!(cache::load_from_path(&path, 88168, 8329).is_none());

    // 2) 写垃圾 JSON → None
    std::fs::write(&path, "not valid json {{").unwrap();
    assert!(cache::load_from_path(&path, 88168, 8329).is_none());

    // 3) 写有效 JSON 但缺字段 → None
    std::fs::write(&path, r#"{"hello":"world"}"#).unwrap();
    assert!(cache::load_from_path(&path, 88168, 8329).is_none());

    let _ = std::fs::remove_file(&path);
}
```

- [ ] **Step 2: 跑测试确认 pass**

`load_from_path` 用 `.ok()?` 早返：IO 失败 / JSON 解析失败都自动转 None。本测应直接 pass。

Run: `cargo test --lib -p sjtu-cli cache_corrupted_returns_none -- --nocapture`
Expected: PASS

- [ ] **Step 3: 跑全量 lib 测兜底**

Run: `cargo test --lib -p sjtu-cli`
Expected: 全绿 + 三个新 cache 测都 pass

- [ ] **Step 4: Commit**

```powershell
git add src/apps/canvas_video/tests_parse.rs
git commit -m "test(canvas_video): cache_corrupted_returns_none 单测（V5.A 步 4）"
```

---

## Task 6: cache.rs 公开 API — `lti_launch_cached` + `save` / `load` / `clear`

把单元逻辑（Task 3-5 已就绪的 save_to_path / load_from_path）包装成 prod 调用面（不带 path 入参，自动算 sub_session 路径）。同时实装 spec 要求的 `clear()`（按 prefix 批量删）。

**Files:**
- Modify: `src/apps/canvas_video/cache.rs`

- [ ] **Step 1: 实装 save / load / lti_launch_cached / clear**

cache.rs 追加（**注意**：放在 chmod_600 上方，保持文件结构合理）：

```rust
// src/apps/canvas_video/cache.rs（追加，置于 load_from_path 之后、chmod_600 之前）

/// prod 调用面：保存到默认 sub_session 路径。失败上抛（save 写不下文件是问题）。
fn save(course_id: u64, lti_tool_id: u64, b: &Bootstrap) -> Result<()> {
    let path = bootstrap_cache_path(course_id, lti_tool_id)?;
    save_to_path(&path, course_id, lti_tool_id, b)
}

/// prod 调用面：从默认 sub_session 路径加载。任一不通过返 None。
fn load(course_id: u64, lti_tool_id: u64) -> Option<Bootstrap> {
    let path = bootstrap_cache_path(course_id, lti_tool_id).ok()?;
    load_from_path(&path, course_id, lti_tool_id)
}

/// 主入口（替代 api.rs 直接调 auth::lti_launch）：
/// - 命中（30min 内 + id 匹配 + JSON ok）→ 直接返
/// - 未命中 → 跑 super::auth::lti_launch + 落盘缓存（save 失败仅 warn 不阻塞）
pub(super) async fn lti_launch_cached(
    course_id: u64,
    lti_tool_id: u64,
) -> Result<Bootstrap> {
    if let Some(cached) = load(course_id, lti_tool_id) {
        debug!(course_id, lti_tool_id, "Bootstrap 缓存命中（TTL 内）");
        return Ok(cached);
    }
    let bootstrap = super::auth::lti_launch(course_id, lti_tool_id).await?;
    if let Err(e) = save(course_id, lti_tool_id, &bootstrap) {
        warn!(?e, course_id, lti_tool_id, "Bootstrap 缓存保存失败，本次仍正常使用");
    }
    Ok(bootstrap)
}

/// 清缓存：
/// - `(Some(course), Some(tool))` → 删该具体文件
/// - `(Some(course), None)` → 删 `canvas_video_bootstrap_<course>_*.json`（多 tool_id 全清）
/// - `(None, _)` → 删所有 `canvas_video_bootstrap_*.json`
///
/// 返回实际删除文件数。文件不存在视作 0 / 1（remove_file 已处理）。
pub(super) fn clear(course_id: Option<u64>, lti_tool_id: Option<u64>) -> Result<u64> {
    let dir = crate::config::sub_sessions_dir()?;
    if !dir.exists() {
        return Ok(0);
    }

    // (Some, Some) 走单文件快路径，没必要 read_dir。
    if let (Some(c), Some(t)) = (course_id, lti_tool_id) {
        let path = bootstrap_cache_path(c, t)?;
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("删除 {} 失败", path.display()))?;
            return Ok(1);
        }
        return Ok(0);
    }

    // 其他情况扫目录按前缀过滤。
    let prefix = match course_id {
        Some(c) => format!("{FILE_PREFIX}{c}_"),
        None => FILE_PREFIX.to_string(),
    };
    let mut count = 0u64;
    for entry in std::fs::read_dir(&dir)
        .with_context(|| format!("read_dir {} 失败", dir.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".json") {
            std::fs::remove_file(entry.path())
                .with_context(|| format!("删除 {:?} 失败", entry.path()))?;
            count += 1;
        }
    }
    Ok(count)
}
```

- [ ] **Step 2: 验证文件行数 + cargo check**

Run（PowerShell）:
```powershell
(Get-Content src\apps\canvas_video\cache.rs | Measure-Object -Line).Lines
```
Expected: 145-160 之间，远低于 200 限。

Run: `cargo check --lib`
Expected: PASS。`save / load / lti_launch_cached / clear` 此时仍未被外部调用 → dead_code warn 是预期的，下两个 task 接入后会消。如果需要立即消 warn，可以临时 `#[allow(dead_code)]` 套在四个 fn 上，Task 7-9 接入后删。**推荐：先忍 warn，Task 7-9 一接入立刻自然消，避免无谓的 attribute churn。**

- [ ] **Step 3: Commit**

```powershell
git add src/apps/canvas_video/cache.rs
git commit -m "feat(canvas_video): cache.rs lti_launch_cached + clear（V5.A 步 5）"
```

---

## Task 7: api.rs `Client::connect` 切到 `cache::lti_launch_cached`

V5.A 的核心一行替换：让 Client::connect 走缓存而不是裸 launch。

**Files:**
- Modify: `src/apps/canvas_video/api.rs:14`（import）
- Modify: `src/apps/canvas_video/api.rs:48`（调用替换）

- [ ] **Step 1: 改 api.rs import + 调用**

```rust
// src/apps/canvas_video/api.rs:14（修改 import）
use super::auth::url_encode_component;
use super::cache;
```

把原来的 `use super::auth::{lti_launch, url_encode_component};` 拆成两行：保留 `url_encode_component` from auth，新增 `use super::cache;`。`lti_launch` 不再直接被 api.rs 引用。

```rust
// src/apps/canvas_video/api.rs:48（修改 connect 调用）
pub async fn connect(course_id: u64, lti_tool_id: u64) -> Result<Self> {
    let bootstrap = cache::lti_launch_cached(course_id, lti_tool_id).await?;
    let http = build_http_client(&bootstrap.session_cookies)?;
    Ok(Self {
        http,
        throttle: Arc::new(Throttle::new()),
        bootstrap,
    })
}
```

变化只是把 `lti_launch(...)` 换成 `cache::lti_launch_cached(...)`。

- [ ] **Step 2: cargo check + clippy**

Run: `cargo check --lib`
Expected: PASS（cache::lti_launch_cached / save / load 现已被引用，dead_code warn 应消除大半）

Run: `cargo clippy --lib --all-targets -- -D warnings`
Expected: PASS

- [ ] **Step 3: 跑全量 lib 测兜底**

Run: `cargo test --lib -p sjtu-cli`
Expected: 全绿（cache 三测 + 既有测）

- [ ] **Step 4: Commit**

```powershell
git add src/apps/canvas_video/api.rs
git commit -m "feat(canvas_video): Client::connect 走 cache::lti_launch_cached（V5.A 步 6）"
```

---

## Task 8: handlers.rs 加 `with_token_refresh` + `looks_like_token_invalid`

业务失败回退 helper。本任务只**新增** helper，不动既有 cmd_list / cmd_download 调用点（那些放 Task 11/12 集中改）。

**Files:**
- Modify: `src/commands/canvas_video/handlers.rs`

- [ ] **Step 1: 加 imports**

handlers.rs 顶部加：

```rust
// src/commands/canvas_video/handlers.rs（顶部 use 区域追加）
use std::future::Future;
use std::sync::Arc;

use crate::apps::canvas_video::cache;
```

注意：`crate::apps::canvas_video::cache` 是私有 mod，但 cache.rs 本身有 `pub(super) fn ...`，外部不可见。需要在 `apps/canvas_video/mod.rs` 改 `mod cache;` → `pub(crate) mod cache;` ⚠️。

**修订**：把 `mod cache;` 改为 `pub(crate) mod cache;` 让 commands 层够得到。同时把 cache.rs 里 `pub(super) fn lti_launch_cached` 改成同样 `pub(super)` 即可（commands 不直接调 lti_launch_cached，只调 clear；clear 是 `pub(super)` —— 也不行，commands 在 super 之外）。

**正确做法**（修补 Task 2 / 6）：cache.rs 里把 commands 层要调的两个 fn `clear` 提到 `pub(crate)` 可见性；`lti_launch_cached` 保持 `pub(super)` 因为只 api.rs 调。同时 mod.rs 的 `mod cache;` 改 `pub(crate) mod cache;`。

**操作**：

修改 `src/apps/canvas_video/mod.rs`：
```rust
pub(crate) mod cache;
```

修改 `src/apps/canvas_video/cache.rs`：把 `pub(super) fn clear` 改为：
```rust
pub(crate) fn clear(course_id: Option<u64>, lti_tool_id: Option<u64>) -> Result<u64> {
```

（lti_launch_cached 保 pub(super) 不动；save/load 私有不动；save_to_path/load_from_path 保 pub(super) 给 tests_parse 用，tests_parse 与 cache 都在 canvas_video 下 super 通。）

- [ ] **Step 2: 实装 with_token_refresh + looks_like_token_invalid**

handlers.rs 文件**末尾**追加：

```rust
// src/commands/canvas_video/handlers.rs（追加）

/// V5.A 业务失败回退：套外面跑一次 op；首次抛 token-invalid 错时清 cache 再跑一次。
/// `looks_like_token_invalid` 决定哪些错触发重试；其他错原封返。
///
/// 用法（cmd_list / cmd_download）：
/// ```ignore
/// with_token_refresh(course_id, tool_id, |client| async move {
///     client.list_lectures(client.cour_id(), client.lti_course_id()).await
/// }).await?
/// ```
pub(super) async fn with_token_refresh<F, Fut, T>(
    course_id: u64,
    lti_tool_id: u64,
    op: F,
) -> Result<T>
where
    F: Fn(Arc<Client>) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let client = Arc::new(Client::connect(course_id, lti_tool_id).await?);
    match op(client.clone()).await {
        Ok(v) => Ok(v),
        Err(e) if looks_like_token_invalid(&e) => {
            tracing::warn!(course_id, lti_tool_id, error = %e, "token 疑作废，清 cache 重试");
            cache::clear(Some(course_id), Some(lti_tool_id))?;
            let client2 = Arc::new(Client::connect(course_id, lti_tool_id).await?);
            op(client2).await
        }
        Err(e) => Err(e),
    }
}

/// 哪些错信号意味着 token 失效该清缓存重 launch。误判成本：多跑一次 ~21s LTI launch；
/// 漏判成本：把过期错原样上抛给用户。前者优于后者，分类宁宽勿严。
fn looks_like_token_invalid(e: &anyhow::Error) -> bool {
    let s = e.to_string();
    s.contains("业务失败 code")
        || s.contains("401")
        || s.contains("403")
        || s.contains("未授权")
}
```

- [ ] **Step 3: cargo check**

Run: `cargo check --lib`
Expected: PASS

with_token_refresh / looks_like_token_invalid 此时仍未被调用 → dead_code warn 是预期的。Task 11-12 接入后消。

- [ ] **Step 4: Commit**

```powershell
git add src/apps/canvas_video/mod.rs src/apps/canvas_video/cache.rs src/commands/canvas_video/handlers.rs
git commit -m "feat(canvas_video): handlers.rs with_token_refresh + cache pub(crate) 暴露（V5.A 步 7）"
```

---

## Task 9: data.rs 加 `ClearCacheData` + handlers.rs 加 `cmd_clear_cache` + commands/mod.rs 暴露

挂 clear-cache 子命令的下半截：handler + envelope data。

**Files:**
- Modify: `src/commands/canvas_video/data.rs`
- Modify: `src/commands/canvas_video/handlers.rs`
- Modify: `src/commands/canvas_video/mod.rs:18-20`

- [ ] **Step 1: data.rs 加 ClearCacheData**

在 data.rs 文件末尾追加：

```rust
// src/commands/canvas_video/data.rs（追加）

/// `sjtu canvas-video clear-cache` 的 envelope data。
#[derive(Debug, Serialize)]
pub(super) struct ClearCacheData {
    /// 实际删除的缓存文件数。
    pub cleared_count: u64,
    /// 范围摘要：`all` 或 `course:<id>`。
    pub scope: String,
}
```

- [ ] **Step 2: handlers.rs 加 cmd_clear_cache**

handlers.rs 顶部 import 区追加：
```rust
use super::data::{ClearCacheData, LectureEntry, ListData};
```
（合并到现有 `use super::data::{LectureEntry, ListData};` 那一行）

handlers.rs 末尾追加 cmd_clear_cache：
```rust
// src/commands/canvas_video/handlers.rs（追加）

/// `sjtu canvas-video clear-cache [--course <id>] [--all]`：清 LTI bootstrap 缓存。
pub async fn cmd_clear_cache(course: Option<u64>, fmt: Option<OutputFormat>) -> Result<()> {
    let cleared = cache::clear(course, None)?;
    let scope = match course {
        Some(c) => format!("course:{c}"),
        None => "all".to_string(),
    };
    render(
        Envelope::ok(ClearCacheData {
            cleared_count: cleared,
            scope,
        }),
        fmt,
    )
}
```

- [ ] **Step 3: commands/canvas_video/mod.rs 暴露 cmd_clear_cache**

```rust
// src/commands/canvas_video/mod.rs:18-20（修改）
pub use batch_handler::{cmd_download_batch, BatchArgs};
pub use download_handler::{cmd_download, cmd_download_all};
pub use handlers::{cmd_clear_cache, cmd_list};
```

- [ ] **Step 4: cargo check**

Run: `cargo check --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```powershell
git add src/commands/canvas_video/data.rs src/commands/canvas_video/handlers.rs src/commands/canvas_video/mod.rs
git commit -m "feat(canvas_video): cmd_clear_cache + ClearCacheData envelope（V5.A 步 8）"
```

---

## Task 10: cli/canvas_video.rs 加 `ClearCache` 子命令 + dispatch

CLI 接入。

**Files:**
- Modify: `src/cli/canvas_video.rs`

- [ ] **Step 1: 加 ClearCache 枚举 variant**

在 CanvasVideoSub 末尾（Download 之后）追加：

```rust
// src/cli/canvas_video.rs（CanvasVideoSub 内追加，置于 Download variant 之后）

    /// 清 LTI bootstrap 缓存。
    /// 默认（无参数）= `--all`：清所有 `canvas_video_bootstrap_*.json`。
    /// `--course <id>`：仅清该课程的所有 tool_id 缓存。
    /// `--all` 与 `--course` 互斥。
    ClearCache {
        /// 仅清指定课程 ID 的缓存。与 `--all` 互斥。
        #[arg(long, conflicts_with = "all")]
        course: Option<u64>,

        /// 显式清所有缓存。与 `--course` 互斥。默认行为已经是 `--all`，本 flag 仅作显式声明。
        #[arg(long, default_value_t = false)]
        all: bool,
    },
```

- [ ] **Step 2: 加 dispatch 分支**

dispatch 函数 match 末尾加：

```rust
// src/cli/canvas_video.rs dispatch 函数 match 块内追加（置于 Download arm 之后）

        CanvasVideoSub::ClearCache { course, all: _ } => {
            // `--all` 仅作显式声明；handler 用 `course == None` 区分 all vs course-specific。
            cv_cmds::cmd_clear_cache(course, fmt).await
        }
```

- [ ] **Step 3: cargo check + clippy**

Run: `cargo check`
Expected: PASS

Run: `cargo clippy --all-targets -- -D warnings`
Expected: PASS

Run: `cargo build --release`
Expected: PASS（confirms binary builds end-to-end）

- [ ] **Step 4: 干跑帮助文本兜底**

Run: `cargo run -- canvas-video clear-cache --help`
Expected: 看到 `--course <COURSE>` 和 `--all` 两个 flag，互斥说明在 clap 自动文案里

- [ ] **Step 5: Commit**

```powershell
git add src/cli/canvas_video.rs
git commit -m "feat(canvas_video): CLI ClearCache 子命令 + dispatch（V5.A 步 9）"
```

---

## Task 11: cmd_list 套 `with_token_refresh`

V5.A 业务失败回退应用到 `sjtu canvas-video list`。

**Files:**
- Modify: `src/commands/canvas_video/handlers.rs:cmd_list`

- [ ] **Step 1: 改 cmd_list 改用 with_token_refresh**

把 `cmd_list` 整体改写：

```rust
// src/commands/canvas_video/handlers.rs:17-66（修改）

/// `sjtu canvas-video list <course_id>`：列一门课的所有讲。
/// V5.A：套 with_token_refresh，业务失败 1 次自动清 cache 重 launch。
pub async fn cmd_list(
    course_id: u64,
    tool_id: u64,
    with_identity: bool,
    include_unaudited: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let (entries, total_raw, cour_id_redacted, lti_course_id_redacted) =
        with_token_refresh(course_id, tool_id, |client| {
            let include_unaudited = include_unaudited;
            let with_identity = with_identity;
            async move {
                let (raw, total_raw) = client
                    .list_lectures(client.cour_id(), client.lti_course_id())
                    .await?;
                let mut filtered: Vec<LectureVideo> = if include_unaudited {
                    raw
                } else {
                    raw.into_iter()
                        .filter(|v| v.vide_audit_status == Some(3))
                        .collect()
                };
                filtered.sort_by(|a, b| {
                    a.course_begin_time
                        .as_deref()
                        .unwrap_or("")
                        .cmp(b.course_begin_time.as_deref().unwrap_or(""))
                });
                let entries: Vec<LectureEntry> = filtered
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| to_entry(i as u32 + 1, v))
                    .collect();
                let cour_id_redacted = redact_or_full(client.cour_id(), with_identity);
                let lti_course_id_redacted =
                    redact_or_full(client.lti_course_id(), with_identity);
                Ok::<_, anyhow::Error>((entries, total_raw, cour_id_redacted, lti_course_id_redacted))
            }
        })
        .await?;

    render(
        Envelope::ok(ListData {
            course_id,
            tool_id,
            with_identity,
            include_unaudited,
            total_raw,
            returned: entries.len(),
            cour_id_redacted,
            lti_course_id_redacted,
            items: entries,
        }),
        fmt,
    )
}
```

注意：闭包内重新绑定 `include_unaudited` / `with_identity` 为局部 `Copy` 变量，是因为外层 Fn 多次调用时不能直接 move 外层值。bool / u32 / i32 都是 Copy 自动 OK；**外层 cmd_list 的 `with_identity` 在 envelope 渲染时还要用**，所以闭包内不能 move 外层的，要 copy 进来。

- [ ] **Step 2: cargo check + clippy**

Run: `cargo check --lib`
Expected: PASS

Run: `cargo clippy --lib --all-targets -- -D warnings`
Expected: PASS

如果遇到 lifetime / borrow 报错：把 `Ok::<_, anyhow::Error>(...)` 的类型推断改成显式 `Ok::<(Vec<LectureEntry>, i64, String, String), anyhow::Error>(...)`。

- [ ] **Step 3: cargo test 兜底**

Run: `cargo test --lib -p sjtu-cli`
Expected: 全绿

- [ ] **Step 4: Commit**

```powershell
git add src/commands/canvas_video/handlers.rs
git commit -m "feat(canvas_video): cmd_list 套 with_token_refresh（V5.A 步 10）"
```

---

## Task 12: cmd_download / cmd_download_all 套 `with_token_refresh`

单讲下载也套，批量不套（spec line 154-156）。

**Files:**
- Modify: `src/commands/canvas_video/download_handler.rs:cmd_download`
- Modify: `src/commands/canvas_video/download_handler.rs:cmd_download_all`

- [ ] **Step 1: cmd_download 改用 with_token_refresh**

`cmd_download` 函数体改写：

```rust
// src/commands/canvas_video/download_handler.rs:cmd_download（修改）

#[allow(clippy::too_many_arguments)]
pub async fn cmd_download(
    course_id: u64,
    tool_id: u64,
    lecture: u32,
    to_dir: PathBuf,
    channel: i32,
    concurrency: usize,
    audio_only: bool,
    keep_mp4: bool,
    with_identity: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let started = Instant::now();
    prep(audio_only, &to_dir).await?;

    use super::handlers::with_token_refresh;
    let (fetch, out, target_video_name, target_video_id) =
        with_token_refresh(course_id, tool_id, |client| {
            let to_dir = to_dir.clone();
            async move {
                let target = super::handlers::resolve_target(&client, lecture).await?;
                let target_video_name = target.video_name.clone();
                let target_video_id = target.video_id.clone();
                let (fetch, out) = download_one_channel(
                    &client,
                    &target,
                    channel,
                    &to_dir,
                    concurrency,
                    audio_only,
                    keep_mp4,
                    with_identity,
                )
                .await?;
                Ok::<_, anyhow::Error>((fetch, out, target_video_name, target_video_id))
            }
        })
        .await?;

    render(
        Envelope::ok(DownloadData {
            course_id,
            tool_id,
            lecture,
            channel: out.channel,
            video_name: fetch.video_name.or(Some(target_video_name)),
            video_id_redacted: super::handlers::redact_or_full(&target_video_id, with_identity),
            duration_secs: fetch.duration_secs,
            file_path: out.file_path,
            audio_path: out.audio_path,
            mp4_kept: out.mp4_kept,
            bytes: out.bytes,
            elapsed_ms: started.elapsed().as_millis(),
            mp4_url_redacted: out.mp4_url_redacted,
        }),
        fmt,
    )
}
```

注意：
- `target` 借用穿不过 async move 重试边界 → 提前 clone `video_name` 与 `video_id` 出来在 envelope 用
- `to_dir` 是 PathBuf 非 Copy，闭包内 `to_dir.clone()` 让每次 op 调用拿独立副本
- `&Client` 接口与 `Arc<Client>` 兼容：`&client` 自动 deref，`download_one_channel(&client, ...)` 直接传 Arc<Client> 的 &T 视图

- [ ] **Step 2: cmd_download_all 同款改写**

```rust
// src/commands/canvas_video/download_handler.rs:cmd_download_all（修改）

#[allow(clippy::too_many_arguments)]
pub async fn cmd_download_all(
    course_id: u64,
    tool_id: u64,
    lecture: u32,
    to_dir: PathBuf,
    concurrency: usize,
    audio_only: bool,
    keep_mp4: bool,
    with_identity: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let started = Instant::now();
    prep(audio_only, &to_dir).await?;

    use super::handlers::with_token_refresh;
    let (channels, video_name, duration_secs, total_bytes, target_video_name, target_video_id) =
        with_token_refresh(course_id, tool_id, |client| {
            let to_dir = to_dir.clone();
            async move {
                let target = super::handlers::resolve_target(&client, lecture).await?;
                let target_video_name = target.video_name.clone();
                let target_video_id = target.video_id.clone();
                let mut channels: Vec<ChannelOutput> = Vec::with_capacity(2);
                let mut video_name: Option<String> = None;
                let mut duration_secs: Option<i64> = None;
                let mut total_bytes = 0u64;
                for ch in [0i32, 1] {
                    let (fetch, out) = download_one_channel(
                        &client,
                        &target,
                        ch,
                        &to_dir,
                        concurrency,
                        audio_only,
                        keep_mp4,
                        with_identity,
                    )
                    .await?;
                    if video_name.is_none() {
                        video_name = fetch.video_name.clone();
                    }
                    if duration_secs.is_none() {
                        duration_secs = fetch.duration_secs;
                    }
                    total_bytes += out.bytes;
                    channels.push(out);
                }
                Ok::<_, anyhow::Error>((
                    channels,
                    video_name,
                    duration_secs,
                    total_bytes,
                    target_video_name,
                    target_video_id,
                ))
            }
        })
        .await?;

    render(
        Envelope::ok(DownloadAllData {
            course_id,
            tool_id,
            lecture,
            video_name: video_name.or(Some(target_video_name)),
            video_id_redacted: super::handlers::redact_or_full(&target_video_id, with_identity),
            duration_secs,
            channels,
            total_bytes,
            total_elapsed_ms: started.elapsed().as_millis(),
        }),
        fmt,
    )
}
```

注意：`for ch in [0i32, 1]` 循环若中途某路 token 失败，**整个 op closure 抛 anyhow → with_token_refresh 看是不是 token-invalid → 是就清 cache 跑第二次，整个双路重头来**。这意味着 `--all-channels` 模式下 ch0 已经下完、ch1 失败时，重试会重下 ch0。CP-V4 的断点续传 check_skip 在 batch handler 里，cmd_download_all 没接，所以 ch0 重下是真的多花一次时间。

**这个边界是可接受的**：cmd_download_all 单讲双路场景罕用（用户多用 batch 走全量）；真要触发也是单讲多 ~80s 而非 50min，OK。

- [ ] **Step 3: cargo check + clippy**

Run: `cargo check --lib`
Expected: PASS

Run: `cargo clippy --lib --all-targets -- -D warnings`
Expected: PASS

- [ ] **Step 4: cargo test 兜底**

Run: `cargo test --lib -p sjtu-cli`
Expected: 全绿

- [ ] **Step 5: 检查文件行数**

Run（PowerShell）:
```powershell
(Get-Content src\commands\canvas_video\download_handler.rs | Measure-Object -Line).Lines
```
Expected: ≤ 200。spec 估 186 → 实际可能 195 上下，仍 OK。如果超 200 → 拆 helper。

- [ ] **Step 6: Commit**

```powershell
git add src/commands/canvas_video/download_handler.rs
git commit -m "feat(canvas_video): cmd_download / cmd_download_all 套 with_token_refresh（V5.A 步 11）"
```

---

## Task 13: cargo fmt + clippy + test 全绿（V5.A 4 关之第 4 关）

**Files:** 无具体 modify，纯校验。

- [ ] **Step 1: cargo fmt**

Run: `cargo fmt --all -- --check`
Expected: PASS（无 diff）

如果 fail：跑 `cargo fmt --all` 让它格式化，再 `cargo fmt --all -- --check` 重验。

- [ ] **Step 2: cargo clippy 全靶 -D warnings**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: PASS（含 tests / examples）

- [ ] **Step 3: cargo test --lib（默认跑 ignored 之外）**

Run: `cargo test --lib -p sjtu-cli`
Expected: 全绿，含 cache 三测

- [ ] **Step 4: 行数大盘**

跑全文件大盘检查 200 行硬限：

```powershell
Get-ChildItem src\apps\canvas_video\*.rs, src\commands\canvas_video\*.rs, src\cli\canvas_video.rs | ForEach-Object {
    $lines = (Get-Content $_ | Measure-Object -Line).Lines
    if ($lines -gt 200) {
        Write-Host "❌ $($_.Name): $lines lines (over 200 limit)"
    } else {
        Write-Host "✓ $($_.Name): $lines lines"
    }
}
```
Expected: 全部 ≤ 200。

- [ ] **Step 5: 不 commit**

本任务是质量门，没新代码不开 commit。所有改动应已在前面 12 个 task 落盘。

---

## Task 14: 真机 4 关验证 + tasks/todo.md + tasks/lessons.md 更新 + 总 commit

V5.A 收口。spec § "真机验证（4 关）"。

**Files:**
- Modify: `tasks/todo.md`
- Modify: `tasks/lessons.md`

- [ ] **Step 1: 关 1 — 第一跑 list 走完整 LTI launch（~21s）**

Run:
```powershell
$started = Get-Date
cargo run --release -- canvas-video list 88168 --yaml
$elapsed = (Get-Date) - $started
Write-Host "elapsed: $($elapsed.TotalSeconds) s"
```
Expected:
- envelope `ok: true`，`items` 共 18 条（all audited）
- elapsed 18-30s（首次 LTI launch + Chrome 启动）

如果 list returned ≠ 18：抓异常环境（学期切换 / 课程结束等），记 lessons.md。

如果 elapsed < 5s：异常 → 缓存可能没真清，跑 `sjtu canvas-video clear-cache --all` 后重做关 1。

- [ ] **Step 2: 关 2 — 第二跑 list 命中缓存（< 1s）**

Run:
```powershell
$started = Get-Date
cargo run --release -- canvas-video list 88168 --yaml
$elapsed = (Get-Date) - $started
Write-Host "elapsed: $($elapsed.TotalSeconds) s"
```
Expected:
- envelope `ok: true`，items 仍 18 条
- elapsed < 1s（缓存命中，跳过 cas_login + Chrome）
- Bytes / 内容应当与关 1 完全一致（除 timestamp 字段）

- [ ] **Step 3: 关 3 — clear-cache + 再跑 list 回到 ~21s**

Run:
```powershell
cargo run --release -- canvas-video clear-cache --all --yaml
```
Expected: envelope `cleared_count >= 1`，`scope: all`

Run:
```powershell
$started = Get-Date
cargo run --release -- canvas-video list 88168 --yaml
$elapsed = (Get-Date) - $started
Write-Host "elapsed after clear: $($elapsed.TotalSeconds) s"
```
Expected: elapsed 回到 18-30s（缓存被清，重 launch）

- [ ] **Step 4: 关 4 — 已在 Task 13 完成（cargo fmt/clippy/test）**

确认 Task 13 已绿即可。

- [ ] **Step 5: 更新 tasks/todo.md**

读 todo.md（应有 V5 章节占位），把 V5.A 标 ✅，记录关 1-3 的实测耗时数：

```markdown
## V5.A LTI bootstrap 缓存（已完成 2026-05-09）
- ✅ cache.rs 实装 + 3 单测全绿
- ✅ Client::connect 走 cache::lti_launch_cached
- ✅ with_token_refresh + cmd_clear_cache CLI
- ✅ 4 关真机：关 1 = <X> s（首次） / 关 2 = <Y> s（命中） / 关 3 = <Z> s（清后再跑）/ 关 4 = fmt+clippy+test 全绿

下一步：V5.B 18 讲全量 audio-only smoke
```

- [ ] **Step 6: 更新 tasks/lessons.md**

加 V5.A 段（lessons.md 末尾），记 1-2 条只在落地中才学到的经验：
- spec 写的"~100 行 cache.rs"实际 ~150 行（因为可测拆分 save_to_path / chmod_600）—— 行数估计的踩坑供后续参考
- with_token_refresh 的 Fn(Arc<Client>) -> Fut trait bound 实战：闭包内重新绑定 Copy 字段、PathBuf 用 .clone()、target struct 提前 clone 字段穿 await 边界
- 关 2 命中实测 <某>ms，对照关 1 的~21s，缓存提速比

如果实装中遇到了非平凡的踩坑（比如 std::fs::rename 在 Windows 跨卷失败 / clippy 抱怨 Arc 包 Send / 某 use 被 cargo udeps 扫出），都加进 lessons.md 用 `## V5.A` 章。

- [ ] **Step 7: 最终 V5.A 单 commit**

```powershell
git add tasks/todo.md tasks/lessons.md
git commit -m "docs(s3g): V5.A LTI bootstrap 缓存收口 — 关 1-4 全绿 + lessons 落地"
```

- [ ] **Step 8: 跑一个完整 git log 兜底**

Run:
```powershell
git log --oneline -15
```
Expected: 看到 V5.A 步 1-12 的 12 commits + 最后这个 docs commit。

V5.A 完成。提示用户：可以触发 V5.B（18 讲 audio-only smoke 后台）+ V5.C（转录调研）的合并 plan 写作了。

---

## Self-Review

**Spec coverage check** — 把 spec § V5.A 每个要点对到 task：

| Spec 要点 | Task |
|---|---|
| 30min TTL + 业务失败回退 | Task 2 (TTL_SECS) + Task 8 (with_token_refresh) |
| cache.rs 新建 ~100 行 | Task 2 + Task 3 + Task 6（实际 ~150 行，已说明溢出原因） |
| auth.rs 不动 | ✅（无 task 触碰） |
| api.rs +1 行 | Task 7 |
| mod.rs +1 行 mod cache | Task 2 + Task 8（修订到 pub(crate)） |
| CachedBootstrap 结构 8 字段 | Task 2 |
| 文件路径 sub_session_path | Task 1（提升至 pub）+ Task 2（bootstrap_cache_path 用） |
| save / load / lti_launch_cached / clear | Task 3 / Task 6 |
| atomic tmp+rename | Task 3（save_to_path 内） |
| 命中 / 失效流程 5 步 | Task 3 / Task 4 / Task 5（覆盖 IO / JSON / id 错配 / TTL） |
| with_token_refresh + looks_like_token_invalid | Task 8 |
| cmd_list / cmd_download 套，cmd_download_batch 不套 | Task 11 / Task 12（不动 batch_handler.rs） |
| clear-cache CLI 子命令 | Task 9 + Task 10 |
| 3 单测 cache_round_trip / cache_expired_returns_none / cache_corrupted_returns_none | Task 3 / Task 4 / Task 5 |
| 真机 4 关 | Task 14（关 1-3）+ Task 13（关 4） |
| 行数预算全 ≤ 200 | Task 13 步 4（行数大盘） |

**Placeholder scan**：仔细扫一遍 plan，所有 code block 都有完整代码，无 `// TODO` / `... existing code ...` 占位。✅

**Type consistency**：
- `cache::clear(course_id, lti_tool_id)` Task 6 签名 `Option<u64>, Option<u64>` → handlers.rs Task 8 调用传 `Some(course_id), Some(lti_tool_id)` ✅
- `cmd_clear_cache(course: Option<u64>, fmt)` Task 9 → CLI Task 10 dispatch arm 传 `course` 一致 ✅
- `with_token_refresh<F, Fut, T>` Task 8 trait bound → cmd_list Task 11 / cmd_download Task 12 调用 site 一致 ✅
- `CachedBootstrap.session_cookies: Vec<Cookie>` Task 2 → save / load Task 3 / lti_launch_cached Task 6 一致 ✅

**Ambiguity check**：
- Task 2 步 3 cargo check 的 dead_code warn 处理留两条路（忍 / 临时 allow），明确推荐第一条 ✅
- Task 8 步 1 关于 `mod cache;` 是 `pub(crate)` 还是 `mod`：明确说"修订 Task 2"，给 fix 操作 ✅
- Task 12 cmd_download_all 重试场景下 ch0 重下：明确说"可接受边界"，不是 bug ✅

无未决问题。Plan 落地。

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-09-v5a-lti-bootstrap-cache.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration. Good fit for V5.A 因为 14 个 task 颗粒度均匀、每个都有明确的 cargo 校验 + commit gate。

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints. 更省启动开销，主对话能即时看到 cargo 输出 / clippy 抱怨。V5.A 各 task 之间耦合不重，inline 也可。

Which approach?
