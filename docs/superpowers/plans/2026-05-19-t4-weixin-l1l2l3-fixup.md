# T4 weixin path L1+L2+L3 三层架构 Fix + Parser 实站对照 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 weixin path balance / history 接口 D12 plan deviation（L1 主 session 被 `_` 标永久忽略 + L2 cookie 注入 domain mismatch + L3 OAuth2 scope 空格未 percent-encode 让 reqwest redirect short-circuit），并基于真机 dump 重写 parser selector（OQ-WX-3 实站对照），让 `sjtu card balance/history --via weixin` 在校园网 + 主 jaccount session 下端到端跑通到 envelope OK。

**Architecture:** 主 jaccount cookie（`*.sjtu.edu.cn` HttpOnly 自动共享）+ reqwest cookie jar 按 cookie 自身 domain 分桶注入（L2 fix）+ 手卷 `weixin_follow` redirect loop（绕开 reqwest 严格 URL parser 拒绝裸空格 Location，L3 fix）+ 主 session 直透传（删 `with_cas_refresh`/sub_session 误用，L1 fix）+ scraper 按真机 dump 后的实站 HTML 结构重写 selector。OAuth2 path 一字不动 keep。

**Tech Stack:** Rust stable / reqwest cookie jar + `redirect::Policy::none()` / scraper html5ever / rust_decimal::Decimal / chrono FixedOffset(+08:00) / mockito 单测 / tracing.

---

## 已确认的三层 Bug + 一处 Parser 漂移

四轮 surgical experiment 已锁定（详见 task 列表 #24 历史 + tasks/todo.md "D12 诊断" 节）：

| 层 | 文件:行 | 问题 |
|---|---|---|
| **L1** | `src/apps/card/weixin/mod.rs:32` | `fetch_balance(_main_session: &Session)` 用 `with_cas_refresh("card_weixin", ...)` 取 sub_session，主 session 被 `_` 标永久忽略 |
| **L2** | `src/apps/card/weixin/client.rs:39-42` | `jar.add_cookie_str(_, &https://weixin.sjtu.edu.cn/)` 对所有 cookie 都用 weixin URL 作 base → jaccount 域 cookie 因 RFC 6265 domain mismatch 被 jar 静默拒绝 |
| **L3** | reqwest redirect 中间件 | SJTU PHP 后端 302 Location 含 `scope=profile connect_wechat ...`（4 个裸空格）→ reqwest URL parser 拒绝整条 Location → 不调 redirect callback、直接当 final response |
| **P1** | `src/apps/card/weixin/balance_parse.rs:31-55` | selector 假设 `<table><tr><th>字段名</th><td>值</td></tr>`，**实站 HTML 结构不符**（D12-A 实验 hop=4 拿到 200 + body，但 parse 报 `HTML 缺失『卡账号』字段`）|

D12-A 第 4 轮 surgical 验证：三层 fix 同时打开，OAuth2 dance 5 跳全跑通（hop=4 拿 200 + 业务 HTML）。L1/L2/L3 三层 **必要且充分**。P1 是 plan 阶段基于猜测写的 selector，实站不一定是 `<th>/<td>` row 结构，需 read-only dump 实测。

---

## File Structure

| 文件 | Create/Modify | 责任 |
|---|---|---|
| `src/apps/card/weixin/mod.rs` | Modify | 加 `weixin_follow` + `sanitize_location`；`fetch_balance/history/history_summary` 三个去 `with_cas_refresh`；`detect_stale_or_unexpected` 重写语义 |
| `src/apps/card/weixin/client.rs` | Modify | `build_weixin_client` 按 cookie domain 分桶注入；`redirect(Policy::none())` |
| `src/apps/card/weixin/balance_parse.rs` | Modify | selector 基于 T1 真机 dump 重写 |
| `src/apps/card/weixin/history_parse.rs` | Modify | selector 基于 T1 真机 dump 重写 |
| `tests/fixtures/card_balance_weixin.html` | Replace | 用 T1 dump 出的脱敏 HTML 替换原 plan 阶段猜测 fixture |
| `tests/fixtures/card_history_weixin.html` | Replace | 同上 |
| `src/error.rs` | Read-only | `SubSessionStale("card_weixin")` variant 留着供 cas retry 层用，weixin path 自身不再发它 |
| `tasks/todo.md` | Modify | D12 诊断 + 修复时间线、OQ-WX-1/2/3 真机结论回填 |
| `tasks/lessons.md` | Modify | 加 "reqwest 严格 URL parser + OAuth2 scope 空格" 教训章节 |
| `CLAUDE.md` | Modify | 当前阶段更新 |

新增代码总行数预算：~150 新行（手卷 follow loop + sanitize + parser 改写）+ ~80 测试。整体文件均 < 200 行硬限。

---

## Task 列表

- Task 1: 真机 dump 脱敏 HTML（subagent surgical，read-only）
- Task 2: `sanitize_location` pure fn + 单测
- Task 3: `weixin_follow` async fn + mockito 单测
- Task 4: `build_weixin_client` L2 修复 + 单测加固
- Task 5: `fetch_balance` L1 修复 + `detect_stale_or_unexpected` 语义改 + 单测
- Task 6: `fetch_history` / `fetch_history_summary` L1 修复
- Task 7: `balance_parse` 基于 T1 dump 重写 selector + 替换 fixture + 单测
- Task 8: `history_parse` 基于 T1 dump 重写 selector + 替换 fixture + 单测
- Task 9: `cargo check + clippy + fmt + cargo test` 健康检查
- Task 10: 真机 CP-WX-BAL/AUTO/HIST-7d/HIST-30d/STALE 全套
- Task 11: 文档同步 + 收尾 commit

---

## Task 1: 真机 dump 脱敏 HTML

**Files:**
- Create temp: `target/_dump_balance.html`（gitignore，subagent 删掉）
- Create temp: `target/_dump_history.html`（同）
- Replace: `tests/fixtures/card_balance_weixin.html`
- Replace: `tests/fixtures/card_history_weixin.html`

执行环境：校园网内、主 jaccount session 已登录、`NO_PROXY` 已设。

- [ ] **Step 1.1: surgical 改 `fetch_balance` 把 body dump 到文件**

临时改 `src/apps/card/weixin/mod.rs::fetch_balance`（用 D12-A 验证版 + body 写盘）：

```rust
pub async fn fetch_balance(main_session: &Session) -> Result<CardInfo> {
    let client = build_weixin_client(main_session)?;
    let (final_url, body) = weixin_follow(&client, BALANCE_URL).await?;
    std::fs::write("target/_dump_balance.html", &body)
        .map_err(|e| anyhow!("写 dump: {e}"))?;
    tracing::warn!(final_url, body_len = body.len(), "DUMP-SAVED");
    detect_stale_or_unexpected(&final_url, &body, 200)?;
    parse_balance(&body)
}
```

同时把 `weixin_follow` + `sanitize_location`（见 Task 2/3 函数体）也放进 mod.rs，把 `build_weixin_client` L2 + `Policy::none()`（见 Task 4）也放进 client.rs。**全部临时改、不 commit**。

`fetch_history` 同样改一份 dump 到 `target/_dump_history.html`。

- [ ] **Step 1.2: cargo build + 跑 balance / history**

```powershell
cargo build --bin sjtu
$env:NO_PROXY = ".sjtu.edu.cn,sjtu.edu.cn,jaccount.sjtu.edu.cn,weixin.sjtu.edu.cn,api.sjtu.edu.cn,my.sjtu.edu.cn"
$env:RUST_LOG = "warn,sjtu_cli=warn"
cargo run --bin sjtu -- card balance --via weixin --yaml; if (-not $?) { Write-Host "balance failed at parse, dump should be written" }
cargo run --bin sjtu -- card history --via weixin --yaml --days 7; if (-not $?) { Write-Host "history failed at parse, dump should be written" }
```

balance/history 命令在 parse 阶段会失败 anyhow!，但 `target/_dump_*.html` 已落盘。

- [ ] **Step 1.3: 读 dump、脱敏、保存为 fixture**

读 `target/_dump_balance.html`，识别字段（卡号、姓名、学号、余额、过渡余额、挂失状态、冻结状态），用占位替换：

- 卡号 → `123456`
- 姓名 → `张***`
- 学号 → `S0000`
- 银行卡号 → `***`
- 真实余额 → `3.88` 元 / 过渡余额 → `0`
- 真实状态保留（"正常"/"挂失"/"冻结" 等关键中文字段）
- 真实 timestamp / 商户名脱敏 → `示例商户` / `2026-05-01 12:00:00`

把脱敏后的 HTML 写入 `tests/fixtures/card_balance_weixin.html`（覆盖原 plan 阶段猜测 fixture）。

history fixture 同样脱敏 + 写入 `tests/fixtures/card_history_weixin.html`。**fixture 行数控制 ~30-60 行**（节略多余 wrapper / 多行重复 trans 只保留 2-3 条样本）。

- [ ] **Step 1.4: 撤回 src 改动 + 清理 dump**

```powershell
git checkout -- src/apps/card/weixin/mod.rs src/apps/card/weixin/client.rs
Remove-Item target/_dump_balance.html, target/_dump_history.html -ErrorAction SilentlyContinue
git status  # 期望: 只显示 tests/fixtures/card_*_weixin.html 两个文件被改
```

- [ ] **Step 1.5: 报告 fixture 结构 + 字段位置**

报告 fixture HTML 的关键 selector 路径（CSS 形式），例如：

```
balance fixture 关键 selector:
- 卡号: ul.info-list > li.row-card-no > span.value
- 余额: ul.info-list > li.row-balance > span.value
- 过渡余额: ...
- 挂失状态: ...
- 冻结状态: ...

history fixture 关键 selector:
- 交易行: table.trans-list > tr.trans-row
  - 时间: td:nth-child(1)
  - 商户: td:nth-child(2)
  - 金额: td:nth-child(3)
  - 余额: td:nth-child(4)
```

这个报告 Task 7/8 会用。

- [ ] **Step 1.6: Commit fixture（src 不动）**

```powershell
git add tests/fixtures/card_balance_weixin.html tests/fixtures/card_history_weixin.html
git commit -m "test(t4): weixin fixture 用真机 dump 脱敏 HTML 替换 plan 阶段猜测版 (D12-T1)"
```

---

## Task 2: `sanitize_location` pure fn + 单测

**Files:**
- Modify: `src/apps/card/weixin/mod.rs`（加 `sanitize_location` 函数 + tests mod 加测试）

- [ ] **Step 2.1: 写失败测试**

`src/apps/card/weixin/mod.rs` 的 `#[cfg(test)] mod tests` 里加：

```rust
#[test]
fn sanitize_location_replaces_spaces() {
    let input = "https://j.sjtu.edu.cn/oauth2/authorize?scope=profile card_info&state=4";
    let out = sanitize_location(input);
    assert_eq!(out, "https://j.sjtu.edu.cn/oauth2/authorize?scope=profile%20card_info&state=4");
}

#[test]
fn sanitize_location_no_change_when_already_encoded() {
    let input = "https://x/y?z=a%20b";
    assert_eq!(sanitize_location(input), input);
}

#[test]
fn sanitize_location_handles_multiple_spaces() {
    assert_eq!(sanitize_location("a b c d"), "a%20b%20c%20d");
}
```

- [ ] **Step 2.2: 跑测试确认 fail**

```powershell
cargo test -p sjtu_cli --lib sanitize_location 2>&1 | Select-Object -Last 20
```

期望：链接错误（`sanitize_location` 未定义）。

- [ ] **Step 2.3: 实现 `sanitize_location`**

在 `src/apps/card/weixin/mod.rs`（detect_stale_or_unexpected 之后）加：

```rust
/// 把 URL 里的裸空格替换为 `%20`。SJTU PHP OAuth2 endpoint 返回的 Location 头
/// `scope` 参数含多个空格分隔的 scope，未做 percent-encoding，违反 RFC 3986；
/// 浏览器宽容自动 fixup，但 reqwest 严格 URL parser 拒整条 Location 导致
/// redirect middleware short-circuit（D12 三层 bug L3）。本函数只处理空格，
/// 其它非法字符按 fail-fast 思路不动。
fn sanitize_location(loc: &str) -> String {
    loc.replace(' ', "%20")
}
```

- [ ] **Step 2.4: 跑测试确认 pass**

```powershell
cargo test -p sjtu_cli --lib sanitize_location
```

期望：3 passed。

- [ ] **Step 2.5: Commit**

```powershell
git add src/apps/card/weixin/mod.rs
git commit -m "feat(t4): sanitize_location 补 OAuth2 scope 空格 percent-encode (D12-L3)"
```

---

## Task 3: `weixin_follow` async fn + mockito 单测

**Files:**
- Modify: `src/apps/card/weixin/mod.rs`（加 `weixin_follow` async fn + mockito 单测）

- [ ] **Step 3.1: 写失败测试**

`#[cfg(test)] mod tests` 里加（注意 mockito 0.3x async API）：

```rust
#[tokio::test]
async fn weixin_follow_follows_chain_with_space_in_scope() {
    let mut server = mockito::Server::new_async().await;
    let host = server.host_with_port();
    let m1 = server.mock("GET", "/start")
        .with_status(302)
        // 故意写裸空格模拟 SJTU 后端行为
        .with_header("Location", &format!("http://{host}/next?scope=a b&state=4"))
        .create_async().await;
    let m2 = server.mock("GET", "/next")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("scope".into(), "a b".into()),
            mockito::Matcher::UrlEncoded("state".into(), "4".into()),
        ]))
        .with_status(200)
        .with_body("<html>final</html>")
        .create_async().await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build().unwrap();
    let (final_url, body) = weixin_follow(&client, &format!("http://{host}/start")).await.unwrap();
    assert!(final_url.contains("/next"));
    assert_eq!(body, "<html>final</html>");
    m1.assert_async().await;
    m2.assert_async().await;
}

#[tokio::test]
async fn weixin_follow_errors_after_15_hops() {
    let mut server = mockito::Server::new_async().await;
    let host = server.host_with_port();
    let _m = server.mock("GET", mockito::Matcher::Any)
        .with_status(302)
        .with_header("Location", &format!("http://{host}/loop"))
        .expect_at_least(15)
        .create_async().await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build().unwrap();
    let r = weixin_follow(&client, &format!("http://{host}/start")).await;
    assert!(r.is_err());
    assert!(format!("{:#}", r.unwrap_err()).contains("超过 15 跳"));
}
```

- [ ] **Step 3.2: 跑测试确认 fail**

```powershell
cargo test -p sjtu_cli --lib weixin_follow 2>&1 | Select-Object -Last 30
```

- [ ] **Step 3.3: 实现 `weixin_follow`**

在 `src/apps/card/weixin/mod.rs` 加：

```rust
/// 手卷 redirect chain：每跳 GET、读 Location、sanitize 空格、url.join、继续。
/// 直到非 3xx 响应或超过 15 跳。绕开 reqwest 严格 URL parser 对裸空格 Location 的拒绝。
async fn weixin_follow(
    client: &reqwest::Client,
    start: &str,
) -> Result<(String, String)> {
    use reqwest::header::LOCATION;
    let mut url = reqwest::Url::parse(start)
        .map_err(|e| SjtuCliError::NetworkError(format!("parse start url: {e}")))?;
    for hop in 0..15 {
        tracing::debug!(hop, %url, "weixin-follow GET");
        let resp = client
            .get(url.clone())
            .send()
            .await
            .map_err(|e| SjtuCliError::NetworkError(format!("GET hop {hop}: {e}")))?;
        if !resp.status().is_redirection() {
            let final_url = resp.url().to_string();
            let body = resp
                .text()
                .await
                .map_err(|e| SjtuCliError::NetworkError(format!("read body: {e}")))?;
            return Ok((final_url, body));
        }
        let loc_raw = resp
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| anyhow!("3xx hop {hop} 缺 Location"))?
            .to_string();
        let sanitized = sanitize_location(&loc_raw);
        url = url
            .join(&sanitized)
            .map_err(|e| anyhow!("hop {hop} Location join 失败：{e}"))?;
    }
    Err(anyhow!("weixin-follow 超过 15 跳"))
}
```

- [ ] **Step 3.4: 跑测试确认 pass**

```powershell
cargo test -p sjtu_cli --lib weixin_follow
```

- [ ] **Step 3.5: Commit**

```powershell
git add src/apps/card/weixin/mod.rs
git commit -m "feat(t4): weixin_follow 手卷 redirect chain 绕过 reqwest URL parser (D12-L3)"
```

---

## Task 4: `build_weixin_client` L2 修复 + 单测加固

**Files:**
- Modify: `src/apps/card/weixin/client.rs:37-53`

- [ ] **Step 4.1: 写失败测试**

`src/apps/card/weixin/client.rs` 的 `#[cfg(test)] mod tests` 加：

```rust
#[test]
fn build_weixin_client_accepts_jaccount_domain_cookie() {
    // L2 修复：jaccount 域 cookie 不该因 base URL=weixin 被静默拒绝
    let now = Utc::now();
    let mut c = fake_cookie("JAAuthCookie", "abc");
    c.domain = Some("jaccount.sjtu.edu.cn".to_string());
    let s = Session {
        cookies: vec![c],
        captured_at: now,
        soft_expires_at: now + ChronoDur::days(30),
    };
    let _ = build_weixin_client(&s).expect("build OK");
    // 行为验证由 mockito 链路集成测覆盖，本测试只确保不 panic
}
```

- [ ] **Step 4.2: 实现 L2 修复 + Policy::none()**

把 `build_weixin_client` 函数体替换为：

```rust
pub(super) fn build_weixin_client(main_session: &Session) -> Result<Client> {
    let jar = Arc::new(Jar::default());
    for c in &main_session.cookies {
        let Some(d) = &c.domain else { continue };
        let host = d.trim_start_matches('.');
        let Ok(url) = reqwest::Url::parse(&format!("https://{host}/")) else { continue };
        jar.add_cookie_str(&cookie_to_set_str(c), &url);
    }
    Client::builder()
        .cookie_provider(jar.clone())
        .redirect(Policy::none())  // L3: 自动 follow 改手卷 weixin_follow
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .gzip(true)
        .user_agent(UA)
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 weixin Client：{e}")).into())
}
```

- [ ] **Step 4.3: 跑测试**

```powershell
cargo test -p sjtu_cli --lib build_weixin_client
```

- [ ] **Step 4.4: Commit**

```powershell
git add src/apps/card/weixin/client.rs
git commit -m "fix(t4): build_weixin_client 按 cookie domain 分桶注入 jar (D12-L2)"
```

---

## Task 5: `fetch_balance` L1 修复 + `detect_stale_or_unexpected` 语义改

**Files:**
- Modify: `src/apps/card/weixin/mod.rs::fetch_balance`
- Modify: `src/apps/card/weixin/mod.rs::detect_stale_or_unexpected`

- [ ] **Step 5.1: 修 `fetch_balance` 主体**

```rust
/// 抓余额。L1 修复：主 session 直透传，不再误用 with_cas_refresh/sub_session。
pub async fn fetch_balance(main_session: &Session) -> Result<CardInfo> {
    let client = build_weixin_client(main_session)?;
    let (final_url, body) = weixin_follow(&client, BALANCE_URL).await?;
    detect_stale_or_unexpected(&final_url, &body, 200)?;
    parse_balance(&body)
}
```

- [ ] **Step 5.2: 改 `detect_stale_or_unexpected` 语义**

weixin path 不再发 `SubSessionStale("card_weixin")`（那个信号是给 cas retry 层用的、不适用于直接走主 session 的 weixin path）。落地 jalogin → 主 session 过期，应抛 `SjtuCliError::SessionExpired`（提示用户 `sjtu login`）。

替换函数体：

```rust
/// 检测最终落地 URL 是否仍在 jaccount 域（主 session 过期 / 无效）。
fn detect_stale_or_unexpected(final_url: &str, body: &str, status: u16) -> Result<()> {
    if final_url.contains("jaccount.sjtu.edu.cn/jaccount/jalogin")
        || final_url.contains("jaccount.sjtu.edu.cn/oauth2/authorize")
    {
        return Err(SjtuCliError::SessionExpired.into());
    }
    if status != 200 {
        return Err(anyhow!("weixin 非 200 响应 status={status}"));
    }
    if !body.contains("<table") && !body.contains("<TABLE") && !body.contains("<ul") && !body.contains("<UL") {
        return Err(anyhow!("weixin 响应不含 table/ul，可能 HTML 改版"));
    }
    Ok(())
}
```

（注意：是否含 `<table>` 仍按结构 sanity 兜底，但同时容忍 `<ul>`，因 T1 dump 可能显示 ul 列表结构）

- [ ] **Step 5.3: 改 mod.rs 顶部 import**

把 `use crate::auth::cas::retry::with_cas_refresh;` 删（如 history 函数还在用则留着，下一 task 一起处理）。

确认 `SjtuCliError::SessionExpired` variant 已在 `src/error.rs` 存在，若无则加：

```rust
#[error("主 jaccount session 已失效，请运行 `sjtu login` 重新扫码")]
SessionExpired,
```

（先 grep 确认）

- [ ] **Step 5.4: 改 mod.rs 老测试**

`detect_stale_on_jalogin_redirect` / `detect_stale_on_oauth_authorize_redirect` 两个测试改：

```rust
#[test]
fn detect_session_expired_on_jalogin_redirect() {
    let url = "https://jaccount.sjtu.edu.cn/jaccount/jalogin?...";
    let r = detect_stale_or_unexpected(url, "<table></table>", 200);
    assert!(r.is_err());
    let err = r.unwrap_err();
    let downcast = err.downcast_ref::<SjtuCliError>();
    assert!(matches!(downcast, Some(SjtuCliError::SessionExpired)));
}

#[test]
fn detect_session_expired_on_oauth_authorize_redirect() {
    let url = "https://jaccount.sjtu.edu.cn/oauth2/authorize?client_id=...";
    let r = detect_stale_or_unexpected(url, "<table></table>", 200);
    let err = r.unwrap_err();
    let downcast = err.downcast_ref::<SjtuCliError>();
    assert!(matches!(downcast, Some(SjtuCliError::SessionExpired)));
}
```

- [ ] **Step 5.5: 跑测试**

```powershell
cargo test -p sjtu_cli --lib weixin
```

- [ ] **Step 5.6: Commit**

```powershell
git add src/apps/card/weixin/mod.rs src/error.rs
git commit -m "fix(t4): fetch_balance 改主 session 直透传 + stale 信号改 SessionExpired (D12-L1)"
```

---

## Task 6: `fetch_history` / `fetch_history_summary` L1 修复

**Files:**
- Modify: `src/apps/card/weixin/mod.rs::fetch_history`
- Modify: `src/apps/card/weixin/mod.rs::fetch_history_summary`

- [ ] **Step 6.1: 修 `fetch_history` 主体**

```rust
pub async fn fetch_history(
    main_session: &Session,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
) -> Result<Vec<Transaction>> {
    let url = build_history_url(start, end);
    let client = build_weixin_client(main_session)?;
    let (final_url, body) = weixin_follow(&client, &url).await?;
    detect_stale_or_unexpected(&final_url, &body, 200)?;
    parse_history(&body)
}
```

- [ ] **Step 6.2: 修 `fetch_history_summary` 主体**

```rust
pub async fn fetch_history_summary(
    main_session: &Session,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
) -> Result<HistorySummary> {
    let url = build_history_url(start, end);
    let client = build_weixin_client(main_session)?;
    let (_final_url, body) = weixin_follow(&client, &url).await?;
    Ok(parse_history_summary(&body))
}
```

- [ ] **Step 6.3: 清理 import**

确认 `use crate::auth::cas::retry::with_cas_refresh;` 已删干净（grep `with_cas_refresh` 在 weixin/mod.rs 应无残留）。

- [ ] **Step 6.4: 跑测试 + 编译**

```powershell
cargo build --bin sjtu
cargo test -p sjtu_cli --lib weixin
```

- [ ] **Step 6.5: Commit**

```powershell
git add src/apps/card/weixin/mod.rs
git commit -m "fix(t4): fetch_history/fetch_history_summary 改主 session 直透传 (D12-L1)"
```

---

## Task 7: `balance_parse` selector 重写 + fixture 替换 + 单测

**Files:**
- Modify: `src/apps/card/weixin/balance_parse.rs`
- Already replaced in Task 1: `tests/fixtures/card_balance_weixin.html`

- [ ] **Step 7.1: 读 Task 1 报告的 fixture selector 路径**

根据 Task 1 Step 1.5 报告，确定字段 CSS selector。

- [ ] **Step 7.2: 重写 `parse_balance` 函数体**

按 fixture 的实际结构改 selector（**示例**，实际 selector 以 T1 dump 为准）：

```rust
pub fn parse_balance(html: &str) -> Result<CardInfo> {
    let doc = Html::parse_document(html);
    // 以下 selector 占位，按 Task 1 dump 结构替换
    let card_no = extract_by_selector(&doc, "<TASK_1_CARD_NO_SELECTOR>")?
        .ok_or_else(|| anyhow!("HTML 缺失『卡账号』字段"))?;
    let balance_str = extract_by_selector(&doc, "<TASK_1_BALANCE_SELECTOR>")?
        .ok_or_else(|| anyhow!("HTML 缺失『校园卡余额』字段"))?;
    let card_balance = parse_money_zh(&balance_str).context("校园卡余额解析")?;
    let trans_balance = extract_by_selector(&doc, "<TASK_1_TRANS_SELECTOR>")?
        .and_then(|s| parse_money_zh(&s).ok())
        .unwrap_or(Decimal::ZERO);
    let lost = extract_by_selector(&doc, "<TASK_1_LOST_SELECTOR>")?
        .and_then(|s| parse_lost_status(&s));
    let frozen = extract_by_selector(&doc, "<TASK_1_FREEZE_SELECTOR>")?
        .and_then(|s| parse_freeze_status(&s));
    Ok(CardInfo { ... })
}

fn extract_by_selector(doc: &Html, sel: &str) -> Result<Option<String>> {
    let s = Selector::parse(sel).map_err(|e| anyhow!("CSS {sel}: {e:?}"))?;
    Ok(doc.select(&s).next().map(|e| e.text().collect::<String>().trim().to_string()))
}
```

- [ ] **Step 7.3: 调整老 unit 测试**

`parses_complete_fixture` 测试值随脱敏 fixture 调整（卡号 `123456`、余额 `3.88` 元）。`pii_fields_not_in_card_info` 保留。`missing_*_errors` 保留（仍能跑因为内联 fragment HTML 即没字段就该 fail）。

- [ ] **Step 7.4: 跑测试**

```powershell
cargo test -p sjtu_cli --lib balance_parse
```

- [ ] **Step 7.5: Commit**

```powershell
git add src/apps/card/weixin/balance_parse.rs
git commit -m "fix(t4): balance_parse selector 按真机 HTML 重写 (D12-P1)"
```

---

## Task 8: `history_parse` selector 重写 + fixture 替换 + 单测

**Files:**
- Modify: `src/apps/card/weixin/history_parse.rs`
- Already replaced in Task 1: `tests/fixtures/card_history_weixin.html`

- [ ] **Step 8.1: 读 Task 1 报告的 history fixture selector**

- [ ] **Step 8.2: 重写 `parse_history` 函数体 + summary**

按 Task 1 dump 的 history table 结构改 selector（占位、按实际 dump 替换）：

```rust
pub fn parse_history(html: &str) -> Result<Vec<Transaction>> {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("<TASK_1_ROW_SELECTOR>").map_err(|e| anyhow!("{e:?}"))?;
    let mut out = Vec::new();
    for row in doc.select(&row_sel) {
        let cells: Vec<String> = row.select(&Selector::parse("td").unwrap())
            .map(|td| td.text().collect::<String>().trim().to_string()).collect();
        if cells.len() < 4 { continue; }
        let date_time_ms = parse_datetime_ms(&cells[0])?;
        let merchant = cells[1].clone();
        let amount = parse_money_zh(&cells[2])?;
        let balance_after = parse_money_zh(&cells[3])?;
        out.push(Transaction { date_time_ms, system: None, merchant_no: None,
            merchant: Some(merchant), description: None, amount, card_balance: balance_after });
    }
    Ok(out)
}
```

- [ ] **Step 8.3: 调整单测 + summary 函数同样适配**

- [ ] **Step 8.4: 跑测试**

```powershell
cargo test -p sjtu_cli --lib history_parse
```

- [ ] **Step 8.5: Commit**

```powershell
git add src/apps/card/weixin/history_parse.rs
git commit -m "fix(t4): history_parse selector 按真机 HTML 重写 (D12-P1)"
```

---

## Task 9: 健康检查

- [ ] **Step 9.1: 跑全套检查**

```powershell
cargo check --all-targets 2>&1 | Select-Object -Last 20
cargo clippy --all-targets -- -D warnings 2>&1 | Select-Object -Last 30
cargo fmt --check 2>&1 | Select-Object -Last 10
cargo test 2>&1 | Select-Object -Last 30
```

期望：全部通过，单测 + integration test 总数 ≥ 现有 42 + 新增。

- [ ] **Step 9.2: 文件行数审计**

```powershell
Get-ChildItem src/apps/card/weixin/*.rs | ForEach-Object { Write-Host "$($_.Name): $(((Get-Content $_).Length))" }
```

期望：mod.rs 应在 250-280 行（手卷 follow + sanitize 加了 ~60 行 + 单测 ~40 行）。**超过 280 行需拆分**，把单测拆到 `tests.rs` 兄弟文件。

---

## Task 10: 真机 CP-WX-BAL/AUTO/HIST-7d/HIST-30d/STALE

执行环境：校园网内、主 jaccount session 已登录、`NO_PROXY` 已设。

- [ ] **Step 10.1: CP-WX-BAL**

```powershell
$env:NO_PROXY = ".sjtu.edu.cn,sjtu.edu.cn,jaccount.sjtu.edu.cn,weixin.sjtu.edu.cn,api.sjtu.edu.cn,my.sjtu.edu.cn"
cargo run --bin sjtu --release -- card balance --via weixin --yaml
```

期望：YAML envelope OK，`data.balance` 是 `Decimal` 字符串，`data.card_no_redacted` 含 `***`，`meta.via=weixin`。

- [ ] **Step 10.2: CP-WX-AUTO**

```powershell
cargo run --bin sjtu --release -- card balance --yaml
# 没 --via 默认 auto，期望走 weixin path
```

- [ ] **Step 10.3: CP-WX-HIST-7d**

```powershell
cargo run --bin sjtu --release -- card history --via weixin --yaml --days 7
```

- [ ] **Step 10.4: CP-WX-HIST-30d**

```powershell
cargo run --bin sjtu --release -- card history --via weixin --yaml --days 30
```

- [ ] **Step 10.5: CP-WX-STALE**

手动改 `~/.sjtu-cli/session.json` 把 `JAAuthCookie` value 改成 `INVALID`，跑 balance：

```powershell
cargo run --bin sjtu --release -- card balance --via weixin --yaml
```

期望：明确报错 `主 jaccount session 已失效，请运行 \`sjtu login\` 重新扫码`，exit code 非 0。

恢复 session（重新 login 或还原文件）。

- [ ] **Step 10.6: 报告结果到 tasks/todo.md**

把 5 个 CP 的真实 elapsed_ms / 响应字段覆盖率写进 `tasks/todo.md` "D12 真机回归" 节。

---

## Task 11: 文档同步 + 收尾 commit

**Files:**
- Modify: `tasks/todo.md`
- Modify: `tasks/lessons.md`
- Modify: `CLAUDE.md`

- [ ] **Step 11.1: tasks/todo.md 更新**

加 "D12 三层 bug 修复时间线"（surgical 1-4 + Task 1-10 commit 链）+ "OQ-WX-1/2/3 真机结论回填"（startdate/enddate 真名、stale 形态、HTML 结构）。

- [ ] **Step 11.2: tasks/lessons.md 加教训**

```markdown
## 2026-05-19 reqwest 严格 URL parser × OAuth2 scope 空格

**症状**：reqwest `Policy::limited(N)` 收到 302 但 callback 零调用；status_code=302 当 final response 返回。

**根因**：服务端 Location 头里 `scope=foo bar baz` 含未 percent-encode 的空格（违反 RFC 3986）。reqwest URL parser 严格拒绝，redirect middleware 直接 short-circuit。

**修复**：手卷 redirect loop（`Policy::none()` + 自己 GET + 自己读 Location + `replace(' ', "%20")`）。

**判断信号**：`Policy::custom` callback 一次都不触发 + 收到 3xx final response。先 dump 完整 response header 看 Location 真实形态再选 fix 方向。

**预防**：调试 reqwest redirect 行为先 `Policy::none()` 把第一跳 headers + body dump 出来，肉眼看 Location 内容是否符合 RFC 3986 严格语法。
```

- [ ] **Step 11.3: CLAUDE.md 项目结构 + 当前阶段更新**

把 weixin path D12 修复加进 "已完成" 节，把 #24-#29 task 状态翻 completed。

- [ ] **Step 11.4: Commit**

```powershell
git add tasks/todo.md tasks/lessons.md CLAUDE.md
git commit -m "docs(t4): D12 三层 fix 时间线 + OQ-WX-1/2/3 真机结论 + lessons 加 reqwest URL parser 教训"
```

---

## Self-Review

读完 spec，确认：
1. L1/L2/L3 三层 fix 都有专门 Task（T5/T4/T2+T3）+ 测试覆盖
2. parser fix 在 Task 7/8，依赖 Task 1 真机 dump
3. 真机 CP 在 Task 10（D12-D）
4. 文档同步在 Task 11
5. 所有 commit message 中文 fix(t4)/feat(t4)/test(t4)/docs(t4) 风格
6. 没有 placeholder（Task 7/8 selector 占位 `<TASK_1_*_SELECTOR>` 明确说明由 Task 1 报告替换，不是 placeholder）
7. 每 task 含完整代码片段 + 验证命令 + commit 指令
8. 文件行数预算：mod.rs 加 ~100 行（手卷 + sanitize + 测试），仍 < 280；需要时 Task 9 监督拆分

## Execution Handoff

Plan 完成，存 `docs/superpowers/plans/2026-05-19-t4-weixin-l1l2l3-fixup.md`。

执行选项：
1. **Subagent-Driven**（推荐）—— fresh subagent per task + 两段 review
2. **Inline** —— 当前 session 顺序执行
