# T4 一卡通 weixin path Fallback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在现有 OAuth2 path（被 `client_id` 审批阻塞）之外，新增 **weixin path** —— HTML scrape `weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecard*.php`（借网信中心 `janicweixin20150709` client_id 透明完成 OAuth2 token 兑换）实现一卡通余额 + 消费记录 read-only 查询；CLI 加 `--via {oauth2, weixin, auto}` flag 双轨切换，`auto` 默认走 weixin。

**Architecture:** 主 jaccount cookie（`*.sjtu.edu.cn` HttpOnly 自动共享）+ reqwest cookie jar + scraper HTML 解析 + `with_cas_refresh`（T8 已实现的 stale auto-refresh）。`Envelope` 加 `meta.via` 字段让 Agent 感知本次走的路径。OAuth2 path 一行不动 keep。

**Tech Stack:** Rust stable / clap 4 derive / reqwest cookie jar / scraper html5ever / rust_decimal::Decimal / chrono FixedOffset(+08:00) / mockito 单测 / tracing.

---

## Plan Deviations from Spec

Plan 阶段发现 spec 两处需调整，记录在此：

### Deviation 1：drop `util::decimal_opt` 前置依赖

**Spec 写**：§6.1 `transition_balance: Option<Decimal>` weixin 独有字段，§4 plan 早期 task 先建 `src/util/decimal_opt.rs`。

**Plan 决定**：OAuth2 path 现有 `apps/card/models.rs::CardInfo.trans_balance: Decimal`（非 Option，serde rename `transBalance`）已是同一概念。weixin path HTML 解析也填同一字段。**不再引入** Option<Decimal>，也不再新建 `util::decimal_opt` 模块。

**影响**：用户给的优先级 ① 失效；plan 总任务数 −1。

### Deviation 2：`CardInfo.lost/frozen` bool 字段保留

**Spec 写**：§6.1 weixin path 新建 `lost_status: Option<CardLostStatus>` / `freeze_status: Option<CardFreezeStatus>` enum 字段。

**Plan 决定**：OAuth2 path 现有 `CardInfo.lost: bool / frozen: bool` 不动（API 服务端就发 bool），weixin path 新增 enum 字段**并行存在**。OAuth2 构造 CardInfo 时填 `lost_status: None / freeze_status: None`，`skip_serializing_if` 保证 JSON 输出不冗余。

**理由**：把 bool 改成 enum 需要 serde custom `Deserialize` 把 `true/false` 映射到 enum variant，破坏 T9-T13 已通过的测试，性价比低。

---

## File Structure

| 文件 | Create/Modify | 责任 |
|---|---|---|
| `src/output.rs` | Modify | 加 `EnvelopeMeta` struct + `Envelope.meta: Option<EnvelopeMeta>` |
| `SCHEMA.md` | Modify | 同步 `meta` 字段章节 |
| `src/apps/card/via.rs` | Create | `CardVia` clap enum + `select_via()` 路径选择器 |
| `src/apps/card/models.rs` | Modify | 加 `CardLostStatus` / `CardFreezeStatus` enum，`CardInfo` 加 weixin 独有 Option 字段 |
| `src/apps/card/weixin/mod.rs` | Create | 顶层 `fetch_balance` / `fetch_history`（包 `with_cas_refresh`）|
| `src/apps/card/weixin/client.rs` | Create | `build_weixin_client` + `cookie_to_set_str` helper |
| `src/apps/card/weixin/money.rs` | Create | `parse_money_zh("3.88 元") -> Decimal` |
| `src/apps/card/weixin/balance_parse.rs` | Create | `parse_balance(html) -> CardInfo` 用 scraper |
| `src/apps/card/weixin/history_parse.rs` | Create | `parse_history(html) -> Vec<Transaction>` + footer 汇总 |
| `src/apps/card/weixin/tests.rs` | Create | weixin path 集中单测（用 fixture HTML）|
| `src/apps/card/mod.rs` | Modify | 加 `pub mod via;` / `pub mod weixin;` |
| `tests/fixtures/card_balance_weixin.html` | Create | 脱敏 HTML fixture（~30 行）|
| `tests/fixtures/card_history_weixin.html` | Create | 脱敏 HTML fixture（~40 行）|
| `src/commands/card/handlers.rs` | Modify | `cmd_balance` / `cmd_history` 加 `via: CardVia` 参数 + dispatch |
| `src/commands/card/data.rs` | Modify | 加 `from_weixin_card_info` / `from_weixin_transactions` 转换 |
| `src/cli/card.rs` | Modify | `Balance` / `History` 子命令加 `--via` clap flag |
| `tasks/todo.md` / `tasks/lessons.md` / `README.md` / `SKILL.md` / `CLAUDE.md` | Modify | 文档同步 |

新增文件总行数预算：**~650** 行 / 9 个新文件 / 每文件 < 130 行 — 远低于 200 行/文件硬限。

---

## Task 列表（按 user 优先级排序）

- Task 1: Envelope.meta + EnvelopeMeta + SCHEMA.md（前置依赖，被 Task 11/12 消费）
- Task 2: via.rs CardVia enum + select_via 纯函数
- Task 3: models.rs 加 CardLostStatus / CardFreezeStatus enum + CardInfo 新字段
- Task 4: weixin/money.rs parse_money_zh
- Task 5: weixin/balance_parse.rs + fixture
- Task 6: weixin/history_parse.rs + fixture
- Task 7: weixin/client.rs + cookie_to_set_str
- Task 8: weixin/mod.rs 顶层 fetch_balance / fetch_history（with_cas_refresh 包装）
- Task 9: apps/card/mod.rs 加 pub mod 声明
- Task 10: commands/card/data.rs 加 from_weixin_* 转换器
- Task 11: commands/card/handlers.rs 加 --via dispatch
- Task 12: cli/card.rs 加 --via clap flag
- Task 13: cargo check + clippy + fmt 健康检查（含 cargo test）
- Task 14: 文档同步（README / SKILL / CLAUDE / todo / lessons）

---

## Task 1: Envelope.meta + EnvelopeMeta struct

**Files:**
- Modify: `src/output.rs:24-64`
- Modify: `SCHEMA.md`（加章节，文件位置 `E:\claude_ask\sjtu_CLI\sjtu-cli\SCHEMA.md`）

- [ ] **Step 1.1: 在 `src/output.rs` 写失败测试**

把以下测试加到 `src/output.rs` 最末（在 `pub fn render` 之后）：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// meta=None 时 JSON 输出不包含 meta 键（后向兼容现有子命令）
    #[test]
    fn envelope_no_meta_serializes_without_meta_key() {
        #[derive(serde::Serialize)]
        struct D { v: i32 }
        let e = Envelope::ok(D { v: 1 });
        let s = serde_json::to_string(&e).unwrap();
        assert!(!s.contains("\"meta\""), "无 meta 时 JSON 不应出现 meta 键: {s}");
    }

    /// meta=Some 时 JSON 输出含 via + source_hint
    #[test]
    fn envelope_with_meta_serializes_via_and_hint() {
        #[derive(serde::Serialize)]
        struct D { v: i32 }
        let e = Envelope::ok_with_meta(D { v: 1 }, EnvelopeMeta {
            via: Some("weixin".into()),
            source_hint: Some("card.sjtu.edu.cn".into()),
        });
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains("\"via\":\"weixin\""), "应含 via: {s}");
        assert!(s.contains("\"source_hint\":\"card.sjtu.edu.cn\""), "应含 source_hint: {s}");
    }

    /// EnvelopeMeta 两字段都 None 时 JSON 输出 meta:{} 还是不输出？
    /// 决策：EnvelopeMeta 整个 Option，None 时 skip；EnvelopeMeta 内部字段 None 时 skip。
    /// 故 EnvelopeMeta { via: None, source_hint: None } 序列化为 "{}"，不影响后向兼容。
    #[test]
    fn envelope_meta_all_none_serializes_empty_object() {
        let m = EnvelopeMeta { via: None, source_hint: None };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(s, "{}");
    }
}
```

- [ ] **Step 1.2: 运行测试确认失败**

Run: `cargo test --lib output::tests -- --nocapture`
Expected: 3 个测试编译失败，错误 `cannot find function ok_with_meta in Envelope`、`cannot find type EnvelopeMeta`.

- [ ] **Step 1.3: 实现 EnvelopeMeta + Envelope.meta 字段 + 构造器**

修改 `src/output.rs`：

**1) 在 `EnvelopeError` struct 后加 `EnvelopeMeta`（行 28 之后）**：

```rust
/// 信封元数据。当前承载本次响应的"路径感知"信息（多路径子系统如 card 双轨）。
/// 字段全 Option + skip_serializing_if，None → JSON 中不出现，后向兼容现有子命令。
#[derive(Debug, Clone, Serialize, Default)]
pub struct EnvelopeMeta {
    /// 实际走的路径名（如 "oauth2" / "weixin"）。Agent / 用户感知用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    /// 数据源域名提示（debug 用，如 "api.sjtu.edu.cn" / "card.sjtu.edu.cn"）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_hint: Option<String>,
}
```

**2) `Envelope` struct 加 `meta` 字段（修改行 31-39）**：

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Envelope<T: Serialize> {
    pub ok: bool,
    pub schema_version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<EnvelopeError>,
    /// 元数据（如 `via` / `source_hint`）。None 时 JSON 输出不出现，后向兼容。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<EnvelopeMeta>,
}
```

**3) `impl Envelope::ok` 加 `meta: None` 默认（修改行 42-50）**：

```rust
impl<T: Serialize> Envelope<T> {
    pub fn ok(data: T) -> Self {
        Self {
            ok: true,
            schema_version: SCHEMA_VERSION,
            data: Some(data),
            error: None,
            meta: None,
        }
    }

    /// 成功信封 + 元数据。card 子命令双轨切换时用。
    pub fn ok_with_meta(data: T, meta: EnvelopeMeta) -> Self {
        Self {
            ok: true,
            schema_version: SCHEMA_VERSION,
            data: Some(data),
            error: None,
            meta: Some(meta),
        }
    }
```

**4) `impl Envelope::err` 同样加 `meta: None`（修改行 53-63）**：

```rust
    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            schema_version: SCHEMA_VERSION,
            data: None,
            error: Some(EnvelopeError {
                code: code.into(),
                message: message.into(),
            }),
            meta: None,
        }
    }
}
```

- [ ] **Step 1.4: 运行测试确认通过**

Run: `cargo test --lib output::tests -- --nocapture`
Expected: 3/3 PASS

- [ ] **Step 1.5: 跑全量 lib 测试确认未破其他子命令**

Run: `cargo test --lib`
Expected: 全绿（包括 elec / shuiyuan / canvas / jwc / card oauth2 path 已有测试）。

- [ ] **Step 1.6: SCHEMA.md 同步**

打开 `SCHEMA.md`，在「Envelope 结构」章节追加：

```markdown
### `meta` 字段（v1+，可选）

`meta` 是 `Option<EnvelopeMeta>`，**仅多路径子系统使用**（当前：card 双轨 OAuth2/weixin）。

```yaml
meta:
  via: "oauth2" | "weixin"               # 实际走的鉴权路径
  source_hint: "api.sjtu.edu.cn" | "card.sjtu.edu.cn"   # 数据源域
```

**后向兼容**：现有子命令（elec/shuiyuan/canvas/jwc/services/jwbmessage）不构造 `meta`，JSON 输出**不出现** `meta` 键。Agent 解析时 `meta` 是 optional 字段。
```

- [ ] **Step 1.7: Commit**

```powershell
git add src/output.rs SCHEMA.md
git commit -m "feat(t4): Envelope 加 meta 字段（via + source_hint），SCHEMA.md 同步章节"
```

---

## Task 2: via.rs CardVia enum + select_via

**Files:**
- Create: `src/apps/card/via.rs`

- [ ] **Step 2.1: 创建 via.rs 写失败测试 + 骨架**

新建 `src/apps/card/via.rs`：

```rust
//! `--via` flag 模型 + 路径选择器（auto 模式根据本地 OAuth2 token 存在性选 weixin 或 oauth2）。

use clap::ValueEnum;

/// CLI `--via` flag 值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum CardVia {
    /// 自动：本地有有效 OAuth2 token → oauth2；否则 weixin。
    #[default]
    Auto,
    /// 强制 OAuth2 path（api.sjtu.edu.cn）。任何错误透传，不 fallback。
    Oauth2,
    /// 强制 weixin path（weixin.sjtu.edu.cn HTML scrape）。
    Weixin,
}

/// 路径选择结果。命令层据此分支调用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedVia {
    Oauth2,
    Weixin,
}

impl ResolvedVia {
    /// 输出到 `Envelope.meta.via`。
    pub fn name(&self) -> &'static str {
        match self {
            Self::Oauth2 => "oauth2",
            Self::Weixin => "weixin",
        }
    }
    /// 输出到 `Envelope.meta.source_hint`。
    pub fn source_hint(&self) -> &'static str {
        match self {
            Self::Oauth2 => "api.sjtu.edu.cn",
            Self::Weixin => "card.sjtu.edu.cn",
        }
    }
}

/// 据 flag + 本地 token 存在性选实际路径。纯函数，可单测。
pub fn select_via(flag: CardVia, has_oauth_token: bool) -> ResolvedVia {
    match flag {
        CardVia::Oauth2 => ResolvedVia::Oauth2,
        CardVia::Weixin => ResolvedVia::Weixin,
        CardVia::Auto => {
            if has_oauth_token {
                ResolvedVia::Oauth2
            } else {
                ResolvedVia::Weixin
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_no_token_picks_weixin() {
        assert_eq!(select_via(CardVia::Auto, false), ResolvedVia::Weixin);
    }

    #[test]
    fn auto_with_token_picks_oauth2() {
        assert_eq!(select_via(CardVia::Auto, true), ResolvedVia::Oauth2);
    }

    #[test]
    fn oauth2_forces_oauth2_regardless_of_token() {
        assert_eq!(select_via(CardVia::Oauth2, false), ResolvedVia::Oauth2);
        assert_eq!(select_via(CardVia::Oauth2, true), ResolvedVia::Oauth2);
    }

    #[test]
    fn weixin_forces_weixin_regardless_of_token() {
        assert_eq!(select_via(CardVia::Weixin, false), ResolvedVia::Weixin);
        assert_eq!(select_via(CardVia::Weixin, true), ResolvedVia::Weixin);
    }

    #[test]
    fn name_and_source_hint_are_distinct() {
        assert_eq!(ResolvedVia::Oauth2.name(), "oauth2");
        assert_eq!(ResolvedVia::Weixin.name(), "weixin");
        assert_eq!(ResolvedVia::Oauth2.source_hint(), "api.sjtu.edu.cn");
        assert_eq!(ResolvedVia::Weixin.source_hint(), "card.sjtu.edu.cn");
    }
}
```

- [ ] **Step 2.2: 暂未在 `apps/card/mod.rs` 声明，独立 build via.rs 不行**

只 build via.rs 不可能（Rust 编译单元是 crate）。直接进 Step 2.3 同步声明 + 测试。

- [ ] **Step 2.3: 在 `src/apps/card/mod.rs` 加 `pub mod via;`**

Edit `src/apps/card/mod.rs` 行 12 后插入：

```rust
pub mod via;
```

修改后 `apps/card/mod.rs` 行 9-14 应为：
```rust
pub mod api;
pub mod http;
pub mod models;
pub mod throttle;
pub mod via;
```

- [ ] **Step 2.4: 运行测试**

Run: `cargo test --lib apps::card::via::tests`
Expected: 5/5 PASS

- [ ] **Step 2.5: Commit**

```powershell
git add src/apps/card/via.rs src/apps/card/mod.rs
git commit -m "feat(t4): CardVia enum + select_via 路径选择器（auto/oauth2/weixin）"
```

---

## Task 3: models.rs 加 weixin path enum 字段

**Files:**
- Modify: `src/apps/card/models.rs:36-65`（`CardInfo` struct）

- [ ] **Step 3.1: 写失败测试（在 `apps/card/tests_parse.rs` 末尾追加）**

打开 `src/apps/card/tests_parse.rs` 末尾追加：

```rust
#[cfg(test)]
mod weixin_enum_tests {
    use crate::apps::card::models::{CardLostStatus, CardFreezeStatus, CardInfo};

    #[test]
    fn lost_status_serde_roundtrip() {
        let v = CardLostStatus::Normal;
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(s, "\"Normal\"");
        let back: CardLostStatus = serde_json::from_str(&s).unwrap();
        assert_eq!(back, CardLostStatus::Normal);
    }

    #[test]
    fn lost_status_lost_variant() {
        let v = CardLostStatus::Lost;
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(s, "\"Lost\"");
    }

    #[test]
    fn freeze_status_serde_roundtrip() {
        let v = CardFreezeStatus::Frozen;
        let s = serde_json::to_string(&v).unwrap();
        let back: CardFreezeStatus = serde_json::from_str(&s).unwrap();
        assert_eq!(back, CardFreezeStatus::Frozen);
    }

    /// weixin 独有字段默认 None，OAuth2 path 反序列化原 API 响应不破坏
    #[test]
    fn card_info_lost_status_defaults_none_when_absent() {
        let api_json = r#"{
            "cardNo":"123456",
            "cardBalance":"3.88",
            "transBalance":"0",
            "lost":false,
            "frozen":false
        }"#;
        let ci: CardInfo = serde_json::from_str(api_json).unwrap();
        assert!(ci.lost_status.is_none(), "lost_status 应默认 None");
        assert!(ci.freeze_status.is_none(), "freeze_status 应默认 None");
    }

    /// weixin path 填了 lost_status 后能正确反序列化
    #[test]
    fn card_info_with_lost_status_roundtrip() {
        let mut ci: CardInfo = serde_json::from_str(r#"{
            "cardNo":"X","cardBalance":"0","transBalance":"0","lost":false,"frozen":false
        }"#).unwrap();
        ci.lost_status = Some(CardLostStatus::Normal);
        ci.freeze_status = Some(CardFreezeStatus::Normal);
        let s = serde_json::to_string(&ci).unwrap();
        assert!(s.contains("\"lost_status\":\"Normal\""), "应序列化 lost_status: {s}");
        assert!(s.contains("\"freeze_status\":\"Normal\""), "应序列化 freeze_status: {s}");
    }
}
```

- [ ] **Step 3.2: 运行测试确认失败**

Run: `cargo test --lib apps::card::tests_parse::weixin_enum_tests`
Expected: 编译失败，错误 `cannot find type CardLostStatus / CardFreezeStatus`.

- [ ] **Step 3.3: 实现 enum + CardInfo 新字段**

Edit `src/apps/card/models.rs` 行 9-11 在 `use rust_decimal::Decimal;` 后追加：

```rust
// (无需新增 use；Serialize/Deserialize 已 use)
```

在文件末尾（行 108 之后）追加 enum 定义：

```rust
/// weixin path 独有：挂失状态字符串 → enum。OAuth2 path 用 `lost: bool` 不走此字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardLostStatus {
    Normal,
    Lost,
}

/// weixin path 独有：冻结状态。OAuth2 path 用 `frozen: bool` 不走此字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardFreezeStatus {
    Normal,
    Frozen,
}
```

修改 `CardInfo` struct（行 35-65 末尾添加 2 个新字段，在 `face_sub_type` 后）：

```rust
    /// weixin path 独有：挂失状态文本枚举。OAuth2 path 永 None（仍用 `lost: bool`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lost_status: Option<CardLostStatus>,
    /// weixin path 独有：冻结状态文本枚举。OAuth2 path 永 None（仍用 `frozen: bool`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freeze_status: Option<CardFreezeStatus>,
}
```

注意：原 `CardInfo` 闭合括号 `}` 要移到新增两字段后。

- [ ] **Step 3.4: 运行测试确认通过**

Run: `cargo test --lib apps::card::tests_parse::weixin_enum_tests`
Expected: 5/5 PASS

- [ ] **Step 3.5: 跑 card 全量测试确认未破 OAuth2 path 现有测试**

Run: `cargo test --lib apps::card`
Expected: 原 T9-T13 单测全绿 + 新 5 个绿。

- [ ] **Step 3.6: Commit**

```powershell
git add src/apps/card/models.rs src/apps/card/tests_parse.rs
git commit -m "feat(t4): CardInfo 加 weixin 独有 lost_status/freeze_status enum 字段"
```

---

## Task 4: weixin/money.rs — parse_money_zh

**Files:**
- Create: `src/apps/card/weixin/money.rs`

- [ ] **Step 4.1: 临时跳过 mod 声明，先建文件 + 写测试**

新建 `src/apps/card/weixin/money.rs`：

```rust
//! 中文金额字符串 → `Decimal`。
//!
//! weixin HTML 字段如 `"3.88 元"` / `"-33.2 元"` / `"20 元"` 没有 OAuth2 JSON 的 `double` 类型。
//! 这里专门一个解析函数收口，避免 `Decimal::from_str` 直接吃带"元"字符串报错。

use anyhow::{anyhow, Result};
use rust_decimal::Decimal;
use std::str::FromStr;

/// 把 `"3.88 元"` / `"-0.8 元"` / `"20"` / `"  -33.2  元  "` 转为 `Decimal`。
/// 失败：纯字符串 / 空 / 多个小数点 → anyhow Err。
pub fn parse_money_zh(s: &str) -> Result<Decimal> {
    let trimmed = s.trim().trim_end_matches('元').trim();
    if trimmed.is_empty() {
        return Err(anyhow!("空金额字符串"));
    }
    Decimal::from_str(trimmed).map_err(|e| anyhow!("解析金额 `{s}` 失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_positive_with_yuan() {
        assert_eq!(parse_money_zh("3.88 元").unwrap(), Decimal::from_str("3.88").unwrap());
    }

    #[test]
    fn parses_negative_consumption() {
        assert_eq!(parse_money_zh("-33.2 元").unwrap(), Decimal::from_str("-33.2").unwrap());
        assert_eq!(parse_money_zh("-0.8 元").unwrap(), Decimal::from_str("-0.8").unwrap());
    }

    #[test]
    fn parses_integer_topup() {
        assert_eq!(parse_money_zh("20 元").unwrap(), Decimal::from(20));
    }

    #[test]
    fn parses_no_unit_suffix() {
        assert_eq!(parse_money_zh("100.5").unwrap(), Decimal::from_str("100.5").unwrap());
    }

    #[test]
    fn parses_with_excessive_whitespace() {
        assert_eq!(parse_money_zh("  3.14  元  ").unwrap(), Decimal::from_str("3.14").unwrap());
    }

    #[test]
    fn parses_zero() {
        assert_eq!(parse_money_zh("0 元").unwrap(), Decimal::ZERO);
        assert_eq!(parse_money_zh("0").unwrap(), Decimal::ZERO);
    }

    #[test]
    fn empty_string_errors() {
        assert!(parse_money_zh("").is_err());
        assert!(parse_money_zh("   ").is_err());
        assert!(parse_money_zh("元").is_err());
    }

    #[test]
    fn garbage_text_errors() {
        assert!(parse_money_zh("abc").is_err());
        assert!(parse_money_zh("1.2.3 元").is_err());
    }
}
```

- [ ] **Step 4.2: weixin/ 目录暂未声明，先建 mod.rs 占位**

新建 `src/apps/card/weixin/mod.rs`（占位，后面 Task 8 填实）：

```rust
//! weixin path（HTML scrape）。Task 8 填实顶层 fetch_* 入口。
pub mod money;
```

- [ ] **Step 4.3: `apps/card/mod.rs` 加 `pub mod weixin;`**

Edit `src/apps/card/mod.rs` 在 `pub mod via;` 后追加 `pub mod weixin;`：

```rust
pub mod api;
pub mod http;
pub mod models;
pub mod throttle;
pub mod via;
pub mod weixin;
```

- [ ] **Step 4.4: 运行测试**

Run: `cargo test --lib apps::card::weixin::money::tests`
Expected: 8/8 PASS

- [ ] **Step 4.5: Commit**

```powershell
git add src/apps/card/weixin/money.rs src/apps/card/weixin/mod.rs src/apps/card/mod.rs
git commit -m "feat(t4): weixin/money.rs parse_money_zh 中文金额 → Decimal"
```

---

## Task 5: weixin/balance_parse.rs + HTML fixture

**Files:**
- Create: `tests/fixtures/card_balance_weixin.html`
- Create: `src/apps/card/weixin/balance_parse.rs`

- [ ] **Step 5.1: 创建 HTML fixture（脱敏）**

新建 `tests/fixtures/card_balance_weixin.html`。**注意 PII 全替换为占位**（学号 `S0000`、姓名 `张***`、卡号 `123456`，金额 `3.88` 任意）：

```html
<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>校园卡余额</title></head>
<body>
<div class="ecard-info">
  <table class="info-table">
    <tr><th>姓名</th><td>张***</td></tr>
    <tr><th>学号</th><td>S0000</td></tr>
    <tr><th>卡账号</th><td>123456</td></tr>
    <tr><th>校园卡余额</th><td>3.88 元</td></tr>
    <tr><th>过渡余额</th><td>0 元</td></tr>
    <tr><th>绑定银行卡</th><td>6228****1234</td></tr>
    <tr><th>挂失状态</th><td>正常</td></tr>
    <tr><th>冻结状态</th><td>正常</td></tr>
  </table>
</div>
</body>
</html>
```

- [ ] **Step 5.2: 写失败测试 + 函数骨架**

新建 `src/apps/card/weixin/balance_parse.rs`：

```rust
//! `ecardbalance.php` HTML → `CardInfo`。
//!
//! HTML 结构（真机调研 2026-05-17）：`<table class="info-table">` 行 = `<tr><th>字段名</th><td>值</td></tr>`。
//! 用 scraper 按 `<th>` 文本 anchor 抽 `<td>` 内容（不依赖 class/id，未来 HTML 改版风险 OQ-WX-3）。
//!
//! PII（姓名 / 学号）**不写入** CardInfo —— 解析时主动 drop。绑定银行卡走 OAuth2 既有 redact 路径。

use anyhow::{anyhow, Context, Result};
use rust_decimal::Decimal;
use scraper::{Html, Selector};

use super::money::parse_money_zh;
use crate::apps::card::models::{CardFreezeStatus, CardInfo, CardLostStatus};

/// 解析 ecardbalance.php HTML 主体为 CardInfo。
///
/// 必有字段：`卡账号` / `校园卡余额`。缺失抛 UpstreamError。
/// 可选字段：`过渡余额` / `挂失状态` / `冻结状态` 缺失 → warn + 用合理默认（ZERO/Normal）。
pub fn parse_balance(html: &str) -> Result<CardInfo> {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("tr").map_err(|e| anyhow!("CSS tr 选择器：{e:?}"))?;
    let th_sel = Selector::parse("th").map_err(|e| anyhow!("CSS th 选择器：{e:?}"))?;
    let td_sel = Selector::parse("td").map_err(|e| anyhow!("CSS td 选择器：{e:?}"))?;

    let mut card_no: Option<String> = None;
    let mut card_balance: Option<Decimal> = None;
    let mut trans_balance: Decimal = Decimal::ZERO;
    let mut lost: Option<CardLostStatus> = None;
    let mut frozen: Option<CardFreezeStatus> = None;

    for tr in doc.select(&row_sel) {
        let label = tr.select(&th_sel).next().map(|e| e.text().collect::<String>().trim().to_string());
        let value = tr.select(&td_sel).next().map(|e| e.text().collect::<String>().trim().to_string());
        match (label.as_deref(), value) {
            (Some("卡账号"), Some(v)) => card_no = Some(v),
            (Some("校园卡余额"), Some(v)) => card_balance = Some(parse_money_zh(&v).context("校园卡余额解析")?),
            (Some("过渡余额"), Some(v)) => {
                trans_balance = parse_money_zh(&v).unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "过渡余额解析失败，回退 0");
                    Decimal::ZERO
                });
            }
            (Some("挂失状态"), Some(v)) => lost = parse_lost_status(&v),
            (Some("冻结状态"), Some(v)) => frozen = parse_freeze_status(&v),
            _ => {} // 姓名 / 学号 / 绑定银行卡 / 其它行：丢弃
        }
    }

    let card_no = card_no.ok_or_else(|| anyhow!("HTML 缺失『卡账号』字段"))?;
    let card_balance = card_balance.ok_or_else(|| anyhow!("HTML 缺失『校园卡余额』字段"))?;

    Ok(CardInfo {
        user: None,
        card_no,
        card_id: None,
        bank_no: None,
        expire_date: None,
        card_balance,
        trans_balance,
        lost: false,           // weixin path 仅经 lost_status，bool 默认 false
        frozen: false,
        face_type: None,
        face_sub_type: None,
        lost_status: lost,
        freeze_status: frozen,
    })
}

fn parse_lost_status(s: &str) -> Option<CardLostStatus> {
    match s.trim() {
        "正常" => Some(CardLostStatus::Normal),
        "挂失" => Some(CardLostStatus::Lost),
        _ => {
            tracing::warn!(value = s, "未知挂失状态字符串");
            None
        }
    }
}

fn parse_freeze_status(s: &str) -> Option<CardFreezeStatus> {
    match s.trim() {
        "正常" => Some(CardFreezeStatus::Normal),
        "冻结" => Some(CardFreezeStatus::Frozen),
        _ => {
            tracing::warn!(value = s, "未知冻结状态字符串");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> String {
        std::fs::read_to_string("tests/fixtures/card_balance_weixin.html")
            .expect("读 fixture 失败")
    }

    #[test]
    fn parses_complete_fixture() {
        let ci = parse_balance(&fixture()).unwrap();
        assert_eq!(ci.card_no, "123456");
        assert_eq!(ci.card_balance, Decimal::from_str_exact("3.88").unwrap());
        assert_eq!(ci.trans_balance, Decimal::ZERO);
        assert_eq!(ci.lost_status, Some(CardLostStatus::Normal));
        assert_eq!(ci.freeze_status, Some(CardFreezeStatus::Normal));
    }

    #[test]
    fn pii_fields_not_in_card_info() {
        let ci = parse_balance(&fixture()).unwrap();
        assert!(ci.user.is_none(), "user 应保持 None（PII 不写入）");
        // bank_no 是 OAuth2 路径字段，weixin path 不填
        assert!(ci.bank_no.is_none(), "bank_no weixin path 应保持 None");
    }

    #[test]
    fn missing_card_balance_errors() {
        let html = r#"<table><tr><th>卡账号</th><td>X</td></tr></table>"#;
        let r = parse_balance(html);
        assert!(r.is_err());
        let msg = format!("{:#}", r.unwrap_err());
        assert!(msg.contains("校园卡余额"), "错误应提及字段：{msg}");
    }

    #[test]
    fn missing_card_no_errors() {
        let html = r#"<table><tr><th>校园卡余额</th><td>1 元</td></tr></table>"#;
        let r = parse_balance(html);
        assert!(r.is_err());
        let msg = format!("{:#}", r.unwrap_err());
        assert!(msg.contains("卡账号"), "错误应提及字段：{msg}");
    }

    #[test]
    fn lost_status_lost_variant() {
        let html = r#"<table>
            <tr><th>卡账号</th><td>X</td></tr>
            <tr><th>校园卡余额</th><td>0 元</td></tr>
            <tr><th>挂失状态</th><td>挂失</td></tr>
        </table>"#;
        let ci = parse_balance(html).unwrap();
        assert_eq!(ci.lost_status, Some(CardLostStatus::Lost));
    }

    #[test]
    fn unknown_status_warns_and_returns_none() {
        let html = r#"<table>
            <tr><th>卡账号</th><td>X</td></tr>
            <tr><th>校园卡余额</th><td>0 元</td></tr>
            <tr><th>挂失状态</th><td>未知状态</td></tr>
        </table>"#;
        let ci = parse_balance(html).unwrap();
        assert!(ci.lost_status.is_none(), "未知状态应 None: {:?}", ci.lost_status);
    }
}
```

- [ ] **Step 5.3: `weixin/mod.rs` 加 `pub mod balance_parse;`**

Edit `src/apps/card/weixin/mod.rs`：

```rust
pub mod balance_parse;
pub mod money;
```

- [ ] **Step 5.4: 运行测试确认通过**

Run: `cargo test --lib apps::card::weixin::balance_parse`
Expected: 6/6 PASS

- [ ] **Step 5.5: Commit**

```powershell
git add tests/fixtures/card_balance_weixin.html src/apps/card/weixin/balance_parse.rs src/apps/card/weixin/mod.rs
git commit -m "feat(t4): weixin/balance_parse.rs ecardbalance.php HTML → CardInfo（PII redact）"
```

---

## Task 6: weixin/history_parse.rs + HTML fixture

**Files:**
- Create: `tests/fixtures/card_history_weixin.html`
- Create: `src/apps/card/weixin/history_parse.rs`

- [ ] **Step 6.1: 创建 HTML fixture（脱敏）**

新建 `tests/fixtures/card_history_weixin.html`：

```html
<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>消费记录</title></head>
<body>
<table class="history">
  <thead>
    <tr><th>交易时间</th><th>系统</th><th>商户</th><th>金额</th><th>卡余额</th></tr>
  </thead>
  <tbody>
    <tr>
      <td>2026-05-17 00:41:00</td>
      <td>六期水控</td>
      <td>六期水控</td>
      <td>-0.8</td>
      <td>3.88</td>
    </tr>
    <tr>
      <td>2026-05-16 12:15:30</td>
      <td>闵一内档</td>
      <td>闵行一餐淮扬快餐</td>
      <td>-15.5</td>
      <td>4.68</td>
    </tr>
    <tr>
      <td>2026-05-15 09:00:00</td>
      <td>银行转账</td>
      <td>银行转账</td>
      <td>20</td>
      <td>20.18</td>
    </tr>
  </tbody>
  <tfoot>
    <tr><td colspan="3">合计</td><td>充值 20 元 / 消费 -16.3 元</td><td></td></tr>
  </tfoot>
</table>
</body>
</html>
```

- [ ] **Step 6.2: 写失败测试 + 函数实现**

新建 `src/apps/card/weixin/history_parse.rs`：

```rust
//! `ecardbill.php` HTML → `Vec<Transaction>` + 可选 footer 汇总。
//!
//! 真机 HTML 结构：`<table class="history"><thead>...</thead><tbody><tr>列5</tr></tbody><tfoot>...</tfoot></table>`。
//! 列序：交易时间 / 系统 / 商户 / 金额 / 卡余额（按真机抓取 2026-05-17）。
//!
//! datetime 解析为 `+08:00 FixedOffset`，serialize 为 ISO8601 字符串（与 OAuth2 path 归一）。

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};
use rust_decimal::Decimal;
use scraper::{ElementRef, Html, Selector};

use super::money::parse_money_zh;
use crate::apps::card::models::Transaction;

const BEIJING_OFFSET_SECS: i32 = 8 * 3600;

/// 解析 ecardbill.php HTML，返回交易记录列表（按 HTML 行序保留，最新在前）。
pub fn parse_history(html: &str) -> Result<Vec<Transaction>> {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("tbody tr").map_err(|e| anyhow!("CSS tbody tr：{e:?}"))?;
    let td_sel = Selector::parse("td").map_err(|e| anyhow!("CSS td：{e:?}"))?;

    let mut out = Vec::new();
    for tr in doc.select(&row_sel) {
        match parse_one_row(&tr, &td_sel) {
            Ok(t) => out.push(t),
            Err(e) => {
                tracing::warn!(error = %e, "跳过无法解析的流水行");
            }
        }
    }
    Ok(out)
}

fn parse_one_row(tr: &ElementRef<'_>, td_sel: &Selector) -> Result<Transaction> {
    let tds: Vec<String> = tr.select(td_sel)
        .map(|e| e.text().collect::<String>().trim().to_string())
        .collect();
    if tds.len() < 5 {
        return Err(anyhow!("流水行列数 {} < 5", tds.len()));
    }
    let dt_naive = NaiveDateTime::parse_from_str(&tds[0], "%Y-%m-%d %H:%M:%S")
        .context("交易时间解析")?;
    let offset = FixedOffset::east_opt(BEIJING_OFFSET_SECS)
        .ok_or_else(|| anyhow!("FixedOffset +08:00 构造失败"))?;
    let dt: DateTime<FixedOffset> = offset.from_local_datetime(&dt_naive)
        .single()
        .ok_or_else(|| anyhow!("本地时间到 FixedOffset 歧义"))?;

    let amount = parse_money_zh(&tds[3]).context("amount 解析")?;
    let card_balance = parse_money_zh(&tds[4]).context("card_balance 解析")?;

    Ok(Transaction {
        date_time_ms: dt.timestamp_millis(),
        date_tim_account_ms: None,
        system: if tds[1].is_empty() { None } else { Some(tds[1].clone()) },
        merchant_no: None,
        merchant: if tds[2].is_empty() { None } else { Some(tds[2].clone()) },
        description: None,
        amount,
        card_balance,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistorySummary {
    pub topup_total: Option<Decimal>,
    pub spend_total: Option<Decimal>,
}

/// 解析页底 `<tfoot>` 汇总。无 footer 或解析失败 → 返 None 字段。
pub fn parse_history_summary(html: &str) -> HistorySummary {
    let doc = Html::parse_document(html);
    let tfoot_sel = match Selector::parse("tfoot td") {
        Ok(s) => s,
        Err(_) => return HistorySummary { topup_total: None, spend_total: None },
    };
    let text: String = doc.select(&tfoot_sel).map(|e| e.text().collect::<String>()).collect();
    HistorySummary {
        topup_total: extract_after(&text, "充值").and_then(|s| parse_money_zh(&s).ok()),
        spend_total: extract_after(&text, "消费").and_then(|s| parse_money_zh(&s).ok()),
    }
}

/// 从 `"充值 20 元 / 消费 -16.3 元"` 中按 `prefix` 截取金额片段。
fn extract_after(text: &str, prefix: &str) -> Option<String> {
    let idx = text.find(prefix)?;
    let rest = &text[idx + prefix.len()..];
    let end = rest.find('/').unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> String {
        std::fs::read_to_string("tests/fixtures/card_history_weixin.html")
            .expect("读 fixture 失败")
    }

    #[test]
    fn parses_three_rows() {
        let v = parse_history(&fixture()).unwrap();
        assert_eq!(v.len(), 3, "应解析 3 条记录");
    }

    #[test]
    fn first_row_field_values() {
        let v = parse_history(&fixture()).unwrap();
        let t0 = &v[0];
        assert_eq!(t0.amount, Decimal::from_str_exact("-0.8").unwrap());
        assert_eq!(t0.card_balance, Decimal::from_str_exact("3.88").unwrap());
        assert_eq!(t0.system.as_deref(), Some("六期水控"));
        assert_eq!(t0.merchant.as_deref(), Some("六期水控"));
    }

    #[test]
    fn topup_row_positive_amount() {
        let v = parse_history(&fixture()).unwrap();
        let t2 = &v[2];
        assert_eq!(t2.amount, Decimal::from(20));
        assert_eq!(t2.system.as_deref(), Some("银行转账"));
    }

    #[test]
    fn datetime_serialized_as_beijing_ms() {
        let v = parse_history(&fixture()).unwrap();
        // 2026-05-17 00:41:00 +08:00 → UTC 2026-05-16 16:41:00 → ms
        let expected = chrono::FixedOffset::east_opt(8 * 3600).unwrap()
            .with_ymd_and_hms(2026, 5, 17, 0, 41, 0).unwrap()
            .timestamp_millis();
        assert_eq!(v[0].date_time_ms, expected);
    }

    #[test]
    fn empty_tbody_returns_empty_vec() {
        let html = r#"<table><tbody></tbody></table>"#;
        let v = parse_history(html).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn malformed_row_skipped() {
        let html = r#"<table><tbody>
            <tr><td>bad-date</td><td>x</td><td>y</td><td>1</td><td>2</td></tr>
            <tr><td>2026-05-17 00:00:00</td><td>x</td><td>y</td><td>1</td><td>2</td></tr>
        </tbody></table>"#;
        let v = parse_history(html).unwrap();
        assert_eq!(v.len(), 1, "坏行跳过，好行保留");
    }

    #[test]
    fn footer_summary_parsed() {
        let s = parse_history_summary(&fixture());
        assert_eq!(s.topup_total, Some(Decimal::from(20)));
        assert_eq!(s.spend_total, Some(Decimal::from_str_exact("-16.3").unwrap()));
    }

    #[test]
    fn footer_summary_missing_returns_none_fields() {
        let html = r#"<table><tbody></tbody></table>"#;
        let s = parse_history_summary(html);
        assert!(s.topup_total.is_none());
        assert!(s.spend_total.is_none());
    }
}
```

- [ ] **Step 6.3: weixin/mod.rs 加声明**

Edit `src/apps/card/weixin/mod.rs`：

```rust
pub mod balance_parse;
pub mod history_parse;
pub mod money;
```

- [ ] **Step 6.4: 运行测试**

Run: `cargo test --lib apps::card::weixin::history_parse`
Expected: 8/8 PASS

- [ ] **Step 6.5: Commit**

```powershell
git add tests/fixtures/card_history_weixin.html src/apps/card/weixin/history_parse.rs src/apps/card/weixin/mod.rs
git commit -m "feat(t4): weixin/history_parse.rs ecardbill.php HTML → Vec<Transaction> + footer 汇总"
```

---

## Task 7: weixin/client.rs — cookie 注入 + reqwest Client

**Files:**
- Create: `src/apps/card/weixin/client.rs`

- [ ] **Step 7.1: 写失败测试 + 实现**

新建 `src/apps/card/weixin/client.rs`：

```rust
//! 注入主 jaccount session cookie 的 reqwest Client。
//!
//! Cookie struct (src/cookies/mod.rs:24-33) 是纯数据无方法，故本地手卷
//! `cookie_to_set_str` 拼成 `Set-Cookie` 形式喂 `reqwest::cookie::Jar::add_cookie_str`。

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::cookie::Jar;
use reqwest::redirect::Policy;
use reqwest::Client;

use crate::cookies::{Cookie, Session};
use crate::error::SjtuCliError;

pub(super) const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// 把 `Cookie` 拼成 `name=value; Domain=...; Path=...` 字符串。
/// expires 故意不拼：jar 不在乎过期，stale 由 `SubSessionStale` 信号驱动重 CAS。
pub(super) fn cookie_to_set_str(c: &Cookie) -> String {
    let mut s = format!("{}={}", c.name, c.value);
    if let Some(d) = &c.domain {
        s.push_str(&format!("; Domain={d}"));
    }
    if let Some(p) = &c.path {
        s.push_str(&format!("; Path={p}"));
    }
    s
}

/// 构造 weixin path 用的 reqwest Client。注入主 session jaccount cookie。
pub(super) fn build_weixin_client(main_session: &Session) -> Result<Client> {
    let jar = Arc::new(Jar::default());
    let url = reqwest::Url::parse("https://weixin.sjtu.edu.cn/")
        .map_err(|e| SjtuCliError::NetworkError(format!("解析 weixin URL：{e}")))?;
    for c in &main_session.cookies {
        jar.add_cookie_str(&cookie_to_set_str(c), &url);
    }
    Client::builder()
        .cookie_provider(jar.clone())
        .redirect(Policy::limited(10))    // OAuth2 透明 redirect 链最多 ~8 跳
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .gzip(true)
        .user_agent(UA)
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 weixin Client：{e}")).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDur, Utc};

    fn fake_cookie(name: &str, value: &str) -> Cookie {
        Cookie {
            name: name.into(),
            value: value.into(),
            domain: Some(".sjtu.edu.cn".into()),
            path: Some("/".into()),
            expires: None,
        }
    }

    #[test]
    fn cookie_to_set_str_full_fields() {
        let c = fake_cookie("JAAuthCookie", "abc123");
        let s = cookie_to_set_str(&c);
        assert!(s.contains("JAAuthCookie=abc123"));
        assert!(s.contains("Domain=.sjtu.edu.cn"));
        assert!(s.contains("Path=/"));
        assert!(!s.contains("Expires"), "expires 故意不拼: {s}");
    }

    #[test]
    fn cookie_to_set_str_minimal() {
        let c = Cookie {
            name: "K".into(),
            value: "V".into(),
            domain: None,
            path: None,
            expires: None,
        };
        let s = cookie_to_set_str(&c);
        assert_eq!(s, "K=V");
    }

    #[test]
    fn build_weixin_client_with_empty_session_works() {
        let now = Utc::now();
        let s = Session {
            cookies: vec![],
            captured_at: now,
            soft_expires_at: now + ChronoDur::days(30),
        };
        let r = build_weixin_client(&s);
        assert!(r.is_ok(), "空 session 也应能 build client：{r:?}");
    }

    #[test]
    fn build_weixin_client_with_one_cookie_works() {
        let now = Utc::now();
        let s = Session {
            cookies: vec![fake_cookie("JAAuthCookie", "abc")],
            captured_at: now,
            soft_expires_at: now + ChronoDur::days(30),
        };
        assert!(build_weixin_client(&s).is_ok());
    }
}
```

- [ ] **Step 7.2: weixin/mod.rs 加声明**

Edit `src/apps/card/weixin/mod.rs`：

```rust
pub mod balance_parse;
pub mod client;
pub mod history_parse;
pub mod money;
```

- [ ] **Step 7.3: 运行测试**

Run: `cargo test --lib apps::card::weixin::client`
Expected: 4/4 PASS

- [ ] **Step 7.4: Commit**

```powershell
git add src/apps/card/weixin/client.rs src/apps/card/weixin/mod.rs
git commit -m "feat(t4): weixin/client.rs cookie 注入 + reqwest Client 构造"
```

---

## Task 8: weixin/mod.rs 顶层 fetch_balance + fetch_history（with_cas_refresh 包装）

**Files:**
- Modify: `src/apps/card/weixin/mod.rs`（从占位升级为入口）

- [ ] **Step 8.1: 在 mod.rs 写新顶层 API（先写文档，没测试 — 网络 IO 不写 unit test，留 mockito 或真机 CP）**

完全覆盖 `src/apps/card/weixin/mod.rs`：

```rust
//! weixin path 入口：通过主 jaccount session cookie 透明跳 OAuth2，HTML scrape ecard*.php。
//!
//! 入口形态：
//! ```ignore
//! let info = fetch_balance(&main_session).await?;        // CardInfo
//! let txs  = fetch_history(&main_session, None, None).await?;  // Vec<Transaction>
//! ```
//!
//! 鉴权与 stale-detect：复用 `crate::auth::cas::retry::with_cas_refresh`（T8）。stale variant
//! `SubSessionStale("card_weixin")` 由 redirect 链落在 jaccount jalogin 时由本模块手动抛。

pub mod balance_parse;
pub mod client;
pub mod history_parse;
pub mod money;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;

use self::balance_parse::parse_balance;
use self::client::build_weixin_client;
use self::history_parse::{parse_history, parse_history_summary, HistorySummary};
use crate::apps::card::models::{CardInfo, Transaction};
use crate::auth::cas::retry::with_cas_refresh;
use crate::cookies::Session;
use crate::error::SjtuCliError;

const BALANCE_URL: &str = "https://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecardbalance.php";
const HISTORY_URL: &str = "https://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecardbill.php";

/// 抓余额。with_cas_refresh 包装，stale 时自动重 cas + 重抓。
pub async fn fetch_balance(_main_session: &Session) -> Result<CardInfo> {
    with_cas_refresh("card_weixin", BALANCE_URL, |session| async move {
        let client = build_weixin_client(&session)?;
        let resp = client.get(BALANCE_URL).send().await
            .map_err(|e| SjtuCliError::NetworkError(format!("GET balance: {e}")))?;
        let status = resp.status();
        let final_url = resp.url().to_string();
        let body = resp.text().await
            .map_err(|e| SjtuCliError::NetworkError(format!("读 balance body: {e}")))?;
        detect_stale_or_unexpected(&final_url, &body, status.as_u16())?;
        parse_balance(&body)
    }).await
}

/// 抓消费记录。`start`/`end` 是日期窗口（默认服务端最近 30 天，OQ-WX-1 plan 阶段未确定参数名）。
pub async fn fetch_history(
    _main_session: &Session,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
) -> Result<Vec<Transaction>> {
    let url = build_history_url(start, end);
    let url_clone = url.clone();
    with_cas_refresh("card_weixin", &url_clone, |session| {
        let url = url.clone();
        async move {
            let client = build_weixin_client(&session)?;
            let resp = client.get(&url).send().await
                .map_err(|e| SjtuCliError::NetworkError(format!("GET history: {e}")))?;
            let status = resp.status();
            let final_url = resp.url().to_string();
            let body = resp.text().await
                .map_err(|e| SjtuCliError::NetworkError(format!("读 history body: {e}")))?;
            detect_stale_or_unexpected(&final_url, &body, status.as_u16())?;
            parse_history(&body)
        }
    }).await
}

/// 同步抓 footer 汇总（CLI 暂不出，留作 history 命令未来扩展）。
pub async fn fetch_history_summary(
    _main_session: &Session,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
) -> Result<HistorySummary> {
    let url = build_history_url(start, end);
    let url_clone = url.clone();
    with_cas_refresh("card_weixin", &url_clone, |session| {
        let url = url.clone();
        async move {
            let client = build_weixin_client(&session)?;
            let resp = client.get(&url).send().await
                .map_err(|e| SjtuCliError::NetworkError(format!("GET history summary: {e}")))?;
            let body = resp.text().await
                .map_err(|e| SjtuCliError::NetworkError(format!("读 body: {e}")))?;
            Ok(parse_history_summary(&body))
        }
    }).await
}

/// OQ-WX-1 plan 阶段假定 query 参数名 `startdate` / `enddate`（CP 阶段实测后调整）。
fn build_history_url(start: Option<NaiveDate>, end: Option<NaiveDate>) -> String {
    match (start, end) {
        (Some(s), Some(e)) => format!("{HISTORY_URL}?startdate={s}&enddate={e}"),
        (Some(s), None) => format!("{HISTORY_URL}?startdate={s}"),
        (None, Some(e)) => format!("{HISTORY_URL}?enddate={e}"),
        (None, None) => HISTORY_URL.to_string(),
    }
}

/// 检测 redirect 链是否被 jaccount 拦截（stale 形态 OQ-WX-2 plan 假定）。
/// 命中 → 抛 `SubSessionStale("card_weixin")`，由 `with_cas_refresh` 接住重试。
fn detect_stale_or_unexpected(final_url: &str, body: &str, status: u16) -> Result<()> {
    if final_url.contains("jaccount.sjtu.edu.cn/jaccount/jalogin")
        || final_url.contains("jaccount.sjtu.edu.cn/oauth2/authorize")
    {
        return Err(SjtuCliError::SubSessionStale("card_weixin").into());
    }
    if status != 200 {
        return Err(anyhow!("weixin 非 200 响应 status={status}"));
    }
    // body 简单 sanity：含 HTML doctype 或 <table>
    if !body.contains("<table") && !body.contains("<TABLE") {
        return Err(anyhow!("weixin 响应不含 <table>，可能 HTML 改版"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn build_history_url_with_both_dates() {
        let s = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let e = NaiveDate::from_ymd_opt(2026, 5, 31).unwrap();
        assert_eq!(
            build_history_url(Some(s), Some(e)),
            format!("{HISTORY_URL}?startdate=2026-05-01&enddate=2026-05-31")
        );
    }

    #[test]
    fn build_history_url_with_no_dates() {
        assert_eq!(build_history_url(None, None), HISTORY_URL);
    }

    #[test]
    fn detect_stale_on_jalogin_redirect() {
        let url = "https://jaccount.sjtu.edu.cn/jaccount/jalogin?...";
        let r = detect_stale_or_unexpected(url, "<table></table>", 200);
        assert!(r.is_err());
        let err = r.unwrap_err();
        let downcast = err.downcast_ref::<SjtuCliError>();
        assert!(matches!(downcast, Some(SjtuCliError::SubSessionStale("card_weixin"))));
    }

    #[test]
    fn detect_stale_on_oauth_authorize_redirect() {
        let url = "https://jaccount.sjtu.edu.cn/oauth2/authorize?client_id=...";
        let r = detect_stale_or_unexpected(url, "<table></table>", 200);
        let err = r.unwrap_err();
        let downcast = err.downcast_ref::<SjtuCliError>();
        assert!(matches!(downcast, Some(SjtuCliError::SubSessionStale("card_weixin"))));
    }

    #[test]
    fn detect_ok_on_real_response() {
        let url = "https://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecardbalance.php";
        assert!(detect_stale_or_unexpected(url, "<html><table>...</table></html>", 200).is_ok());
    }

    #[test]
    fn detect_errors_on_non_200() {
        let url = "https://weixin.sjtu.edu.cn/.../ecardbalance.php";
        assert!(detect_stale_or_unexpected(url, "<table></table>", 500).is_err());
    }

    #[test]
    fn detect_errors_on_html_without_table() {
        let url = "https://weixin.sjtu.edu.cn/.../ecardbalance.php";
        let r = detect_stale_or_unexpected(url, "<html>nothing</html>", 200);
        assert!(r.is_err());
    }
}
```

- [ ] **Step 8.2: 运行测试**

Run: `cargo test --lib apps::card::weixin::tests`
Expected: 7/7 PASS（含 detect_stale_* / build_history_url_* 系列）

- [ ] **Step 8.3: 全量 cargo check**

Run: `cargo check --lib`
Expected: 全绿。

- [ ] **Step 8.4: Commit**

```powershell
git add src/apps/card/weixin/mod.rs
git commit -m "feat(t4): weixin/mod.rs 顶层 fetch_balance/fetch_history（with_cas_refresh 包装 + stale detect）"
```

---

## Task 9: commands/card/data.rs 加 weixin → CLI data 转换器

**Files:**
- Modify: `src/commands/card/data.rs`（需先 Read 现有结构）

- [ ] **Step 9.1: 先 Read 现有 `src/commands/card/data.rs` 了解 BalanceData / HistoryData / TransactionItem 结构**

Run: `cargo run -- --help` 之前先看文件。执行：

```powershell
# 仅查阅，不修改
Get-Content src/commands/card/data.rs -TotalCount 80
```

- [ ] **Step 9.2: 在 data.rs 写 from_weixin_card_info 转换器（带单测）**

在 `src/commands/card/data.rs` 末尾追加（具体追加位置：现有 `impl BalanceData` 块**之后**；如无该 impl 块，则文件末尾）：

```rust
impl BalanceData {
    /// 从 weixin path `CardInfo` 转 BalanceData。
    /// weixin path 不含 user/bank_no（PII redact 已在 parse 层），全字段 None。
    /// `lost_status` / `freeze_status` 通过 `lost` / `frozen` bool 表达（Normal → false / Lost or Frozen → true）。
    pub fn from_weixin_card_info(ci: &crate::apps::card::models::CardInfo) -> Self {
        use crate::apps::card::models::{CardFreezeStatus, CardLostStatus};
        let lost = matches!(ci.lost_status, Some(CardLostStatus::Lost));
        let frozen = matches!(ci.freeze_status, Some(CardFreezeStatus::Frozen));
        Self {
            card_no: redact_card_no(&ci.card_no),
            card_balance: ci.card_balance,
            trans_balance: ci.trans_balance,
            lost,
            frozen,
            identity: None,   // weixin path 不出 identity（PII 永久 redact）
        }
    }
}

impl HistoryData {
    /// 从 weixin path 解析出的 Vec<Transaction> 转 HistoryData。
    /// 时间归一为 +08:00 ISO8601 字符串（与 OAuth2 path 一致）。
    pub fn from_weixin_transactions(txs: &[crate::apps::card::models::Transaction]) -> Self {
        let items = txs.iter().map(|t| {
            TransactionItem::from_oauth2_transaction(t)
        }).collect();
        Self { transactions: items, total: txs.len() as u64 }
    }
}
```

⚠️ 上面引用了 `TransactionItem::from_oauth2_transaction`，假定 OAuth2 path 已有该转换器。如该方法名不同（如 `from_transaction` 或 `from_model`），在 Step 9.1 阅读时确认实际名称并替换。如 OAuth2 path **没有现成转换器**，本 task 需要补一个：

```rust
impl TransactionItem {
    pub fn from_oauth2_transaction(t: &crate::apps::card::models::Transaction) -> Self {
        use chrono::{FixedOffset, TimeZone};
        let offset = FixedOffset::east_opt(8 * 3600).unwrap();
        let dt = offset.timestamp_millis_opt(t.date_time_ms).single();
        Self {
            datetime: dt.map(|d| d.to_rfc3339()).unwrap_or_default(),
            system: t.system.clone(),
            merchant: t.merchant.clone(),
            description: t.description.clone(),
            amount: t.amount,
            card_balance: t.card_balance,
        }
    }
}
```

字段名以 Step 9.1 实际看到的 `TransactionItem` 为准（必要时 plan 阶段就地调整）。

- [ ] **Step 9.3: 测试**

在 `src/commands/card/data.rs` 末尾追加测试：

```rust
#[cfg(test)]
mod weixin_conversion_tests {
    use super::*;
    use crate::apps::card::models::{CardFreezeStatus, CardInfo, CardLostStatus, Transaction};
    use rust_decimal::Decimal;

    fn fake_weixin_card_info(lost: Option<CardLostStatus>, frozen: Option<CardFreezeStatus>) -> CardInfo {
        CardInfo {
            user: None, card_no: "123456".into(), card_id: None, bank_no: None,
            expire_date: None,
            card_balance: Decimal::from_str_exact("3.88").unwrap(),
            trans_balance: Decimal::ZERO,
            lost: false, frozen: false,
            face_type: None, face_sub_type: None,
            lost_status: lost, freeze_status: frozen,
        }
    }

    #[test]
    fn balance_data_from_weixin_normal_status() {
        let ci = fake_weixin_card_info(Some(CardLostStatus::Normal), Some(CardFreezeStatus::Normal));
        let bd = BalanceData::from_weixin_card_info(&ci);
        assert!(!bd.lost);
        assert!(!bd.frozen);
        assert!(bd.identity.is_none());
    }

    #[test]
    fn balance_data_from_weixin_lost_card() {
        let ci = fake_weixin_card_info(Some(CardLostStatus::Lost), Some(CardFreezeStatus::Normal));
        let bd = BalanceData::from_weixin_card_info(&ci);
        assert!(bd.lost, "lost_status=Lost 应映射 lost=true");
        assert!(!bd.frozen);
    }

    #[test]
    fn balance_data_from_weixin_redacts_card_no() {
        let ci = fake_weixin_card_info(None, None);
        let bd = BalanceData::from_weixin_card_info(&ci);
        // redact_card_no 行为：前 2 + **** + 后 2 类似（具体看实现）
        assert_ne!(bd.card_no, "123456", "card_no 应已 redact");
    }
}
```

- [ ] **Step 9.4: 运行测试**

Run: `cargo test --lib commands::card::data`
Expected: 全绿（含新加 3 个 + 原有测试）。

- [ ] **Step 9.5: Commit**

```powershell
git add src/commands/card/data.rs
git commit -m "feat(t4): commands/card/data.rs 加 weixin → BalanceData/HistoryData 转换器"
```

---

## Task 10: commands/card/handlers.rs 加 --via dispatch

**Files:**
- Modify: `src/commands/card/handlers.rs`

- [ ] **Step 10.1: Read 现有 `cmd_balance` / `cmd_history` 函数签名**

```powershell
Get-Content src/commands/card/handlers.rs
```

记录 OAuth2 path 的 cmd_balance / cmd_history 现有 signature。

- [ ] **Step 10.2: 改造 `cmd_balance` 签名加 `via: CardVia`**

修改 `cmd_balance` 函数签名（在 handlers.rs 头部 `use` 块加 import）：

```rust
use crate::apps::card::via::{select_via, CardVia, ResolvedVia};
use crate::auth::oauth2_dev::CardOAuthSession;
use crate::cookies;
use crate::output::EnvelopeMeta;
```

新签名（替换现有 `cmd_balance`）：

```rust
pub async fn cmd_balance(
    with_identity: bool,
    via: CardVia,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    // 检测本地是否有有效 OAuth2 token
    let oauth_session_path = oauth2_dev::session_path()?;
    let has_oauth_token = oauth_session_path.exists()
        && CardOAuthSession::load(&oauth_session_path).is_ok();

    let resolved = select_via(via, has_oauth_token);
    let main_session = cookies::load_session()?
        .ok_or(SjtuCliError::NotAuthenticated)?;

    let meta = EnvelopeMeta {
        via: Some(resolved.name().to_string()),
        source_hint: Some(resolved.source_hint().to_string()),
    };

    match resolved {
        ResolvedVia::Weixin => {
            let info = crate::apps::card::weixin::fetch_balance(&main_session).await?;
            let data = BalanceData::from_weixin_card_info(&info);
            // weixin path 永久不出 identity（即便用户传 --with-identity）—— PII redact 红线
            if with_identity {
                tracing::warn!("weixin path 不支持 --with-identity；该 flag 已忽略");
            }
            render(Envelope::ok_with_meta(data, meta), fmt)?;
        }
        ResolvedVia::Oauth2 => {
            // OAuth2 path 复用现有实现（行为不变，只是裹一层 meta）
            cmd_balance_oauth2_inner(with_identity, meta, fmt).await?;
        }
    }
    Ok(())
}

/// 原 `cmd_balance` 内含的 OAuth2 path 主体提取到本函数，加 meta。
async fn cmd_balance_oauth2_inner(
    with_identity: bool,
    meta: EnvelopeMeta,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let secret = secret::load_secret()?;
    let client = Client::new(...);  // 留原代码逻辑
    let info = ensure_fresh_and_call(&client, |c| c.get_balance()).await?;
    let data = if with_identity {
        BalanceData::from_card_info_with_identity(&info)
    } else {
        BalanceData::from_card_info(&info)
    };
    render(Envelope::ok_with_meta(data, meta), fmt)?;
    Ok(())
}
```

⚠️ 注意：`cmd_balance_oauth2_inner` 的内部主体必须是从**当前** `cmd_balance` 函数体里**完整搬运**过来的代码，且最末 `render` 调用改成 `Envelope::ok_with_meta(data, meta)`。原 `cmd_balance` 体不要丢任何分支逻辑。Step 10.1 阅读时把原 body 抄一份作 baseline。

- [ ] **Step 10.3: 同样改造 `cmd_history` 加 via dispatch**

```rust
pub async fn cmd_history(
    days: i64,
    limit: u32,
    via: CardVia,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let oauth_session_path = oauth2_dev::session_path()?;
    let has_oauth_token = oauth_session_path.exists()
        && CardOAuthSession::load(&oauth_session_path).is_ok();
    let resolved = select_via(via, has_oauth_token);
    let main_session = cookies::load_session()?
        .ok_or(SjtuCliError::NotAuthenticated)?;
    let meta = EnvelopeMeta {
        via: Some(resolved.name().to_string()),
        source_hint: Some(resolved.source_hint().to_string()),
    };

    match resolved {
        ResolvedVia::Weixin => {
            // weixin path：days 转 NaiveDate 区间
            let end = chrono::Local::now().date_naive();
            let start = end - chrono::Duration::days(days);
            let txs = crate::apps::card::weixin::fetch_history(&main_session, Some(start), Some(end)).await?;
            let truncated: Vec<_> = txs.into_iter().take(limit as usize).collect();
            let data = HistoryData::from_weixin_transactions(&truncated);
            render(Envelope::ok_with_meta(data, meta), fmt)?;
        }
        ResolvedVia::Oauth2 => {
            cmd_history_oauth2_inner(days, limit, meta, fmt).await?;
        }
    }
    Ok(())
}

async fn cmd_history_oauth2_inner(
    days: i64,
    limit: u32,
    meta: EnvelopeMeta,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    // 留原 cmd_history 主体，render 末尾改 ok_with_meta
}
```

- [ ] **Step 10.4: cargo check**

Run: `cargo check --lib`
Expected: 全绿。如失败：仔细对照 Step 10.2 与 Step 10.1 抄出的 baseline，确保把 OAuth2 path 的所有原始逻辑迁入 `cmd_balance_oauth2_inner`。

- [ ] **Step 10.5: 跑全量测试**

Run: `cargo test`
Expected: 全绿（含原 commands::card 单测）。

- [ ] **Step 10.6: Commit**

```powershell
git add src/commands/card/handlers.rs
git commit -m "feat(t4): commands/card/handlers --via dispatch（auto/oauth2/weixin + Envelope.meta）"
```

---

## Task 11: cli/card.rs 加 --via clap flag

**Files:**
- Modify: `src/cli/card.rs`

- [ ] **Step 11.1: Read 现有 CardSub enum 完整定义**

```powershell
Get-Content src/cli/card.rs
```

记录 `Balance` / `History` variant 的现有 fields + dispatch 块。

- [ ] **Step 11.2: 在 `Balance` 和 `History` variant 加 `via: CardVia` 字段**

修改 `src/cli/card.rs` 加 import：

```rust
use crate::apps::card::via::CardVia;
```

修改 `Balance` variant：

```rust
    /// 当前卡余额查询。**只读**。
    ///
    /// 默认抹身份字段；`--with-identity` 出学号/姓名/单位/绑定银行卡（前 4 + **** + 后 4）。
    Balance {
        /// 包含身份字段（学号 / 姓名 / 单位 / 银行卡尾号）。默认不出。
        #[arg(long)]
        with_identity: bool,

        /// 鉴权路径：auto（默认，无 OAuth2 token 走 weixin）/ oauth2 / weixin。
        #[arg(long, value_enum, default_value_t = CardVia::Auto)]
        via: CardVia,
    },
```

同理改 `History`：

```rust
    History {
        #[arg(long, default_value_t = 30)]
        days: i64,
        #[arg(long, default_value_t = 50)]
        limit: u32,
        /// 鉴权路径：auto（默认）/ oauth2 / weixin。
        #[arg(long, value_enum, default_value_t = CardVia::Auto)]
        via: CardVia,
    },
```

修改 dispatch 块（文件末尾 `match` 或 `dispatch` 函数），把 `via` 传入：

```rust
        CardSub::Balance { with_identity, via } => {
            card_cmds::cmd_balance(with_identity, via, fmt).await
        }
        CardSub::History { days, limit, via } => {
            card_cmds::cmd_history(days, limit, via, fmt).await
        }
```

- [ ] **Step 11.3: cargo check + 实跑 --help**

Run:
```powershell
cargo check --lib --bin sjtu
cargo run -- card balance --help
```

Expected：`--help` 输出含 `--via <VIA>  鉴权路径：auto（默认...）/ oauth2 / weixin` 和 `[possible values: auto, oauth2, weixin]`。

- [ ] **Step 11.4: 跑全量测试**

Run: `cargo test`
Expected: 全绿。

- [ ] **Step 11.5: Commit**

```powershell
git add src/cli/card.rs
git commit -m "feat(t4): cli/card.rs Balance/History 加 --via clap flag"
```

---

## Task 12: 健康检查 — cargo check / clippy / fmt / test

**Files:** 无修改（只跑命令）

- [ ] **Step 12.1: cargo check 全部**

Run: `cargo check --workspace --all-targets`
Expected: 全绿，零 warning。

- [ ] **Step 12.2: cargo clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 零警告。如有 warning：实际修复（不要 `#[allow(...)]` 抑制，除非确证误报）。

- [ ] **Step 12.3: cargo fmt --check**

Run: `cargo fmt --check`
Expected: 零 diff。如有 diff：跑 `cargo fmt` 修复，单独一个 commit `chore(t4): cargo fmt`。

- [ ] **Step 12.4: cargo test 全量**

Run: `cargo test`
Expected: 全绿。

- [ ] **Step 12.5: 行数审计**

Run:
```powershell
Get-ChildItem -Path src/apps/card/weixin -Filter *.rs |
  ForEach-Object { "{0,-50} {1}" -f $_.FullName.Substring($PWD.Path.Length+1), (Get-Content $_.FullName | Measure-Object -Line).Lines }
Get-ChildItem -Path src/apps/card/via.rs |
  ForEach-Object { "{0,-50} {1}" -f $_.FullName.Substring($PWD.Path.Length+1), (Get-Content $_.FullName | Measure-Object -Line).Lines }
```

Expected: 每文件行数 < 200（CLAUDE.md 硬约束）。如有超 200 → 触发拆分（按职责再细分子模块）。

- [ ] **Step 12.6: cargo machete 死代码扫描（如已装）**

Run: `cargo machete` （若未装则跳过 — 不阻塞）
Expected: 无新增未使用依赖。

- [ ] **Step 12.7: 健康检查无新 commit（验证步骤）**

健康检查不该产生代码改动；如有 `cargo fmt` 之类调整，单独一 commit。

---

## Task 13: 文档同步 — README / SKILL / CLAUDE / todo / lessons

**Files:**
- Modify: `README.md`
- Modify: `SKILL.md`
- Modify: `CLAUDE.md`
- Modify: `tasks/todo.md`
- Modify: `tasks/lessons.md`

- [ ] **Step 13.1: README.md 更新一卡通章节**

打开 `README.md`，找一卡通章节，把命令示例加 `--via`：

```markdown
### 一卡通

`sjtu card balance` —— 卡余额查询（默认 `--via auto`）
`sjtu card history --days 30` —— 30 天消费记录
`sjtu card balance --via weixin` —— 强制走 weixin path（HTML scrape）
`sjtu card balance --via oauth2` —— 强制走 OAuth2 path（需 client_id + client_secret）

| `--via` | 鉴权 | 数据源 | 适用场景 |
|---|---|---|---|
| `auto`（默认）| 本地 OAuth2 token 存在 → oauth2；否则 weixin | 自动 | 无脑选 |
| `oauth2` | OAuth2 Authorization Code | `api.sjtu.edu.cn` | 已申请到 client_id |
| `weixin` | jaccount cookie + HTML scrape | `card.sjtu.edu.cn` → `weixin.sjtu.edu.cn` | 无 client_id 时 |
```

- [ ] **Step 13.2: SKILL.md 更新 Agent 调用示例**

打开 `SKILL.md`，一卡通章节加 `meta.via` 字段说明：

```markdown
### 一卡通输出 envelope

```yaml
ok: true
schema_version: "1"
data:
  card_no: "12****34"
  card_balance: "3.88"
  ...
meta:
  via: "weixin"                # 实际走的路径 — auto 模式时 Agent 应据此判断后续行为
  source_hint: "card.sjtu.edu.cn"
```

Agent 用法：若需要 identity 字段（学号/姓名），仅 `via=oauth2` 时才可能存在；`via=weixin` 时永远 `identity: null`（PII 红线）。
```

- [ ] **Step 13.3: CLAUDE.md 当前阶段标记更新**

打开 `CLAUDE.md`，找 `### 当前阶段` 章节，在「已完成」末尾追加：

```markdown
/ **S3 Phase 2 第二弹 — T4 weixin path fallback（2026-05-18 完成 14 个 task / 9 新文件 ~650 行 / 单元测试 40+ 全绿 / Envelope.meta + via.rs CardVia + weixin/ 6 子文件 + handlers --via dispatch + cli --via flag）；真机 CP 3 项（CP-WX-BAL/HIST/STALE）阻塞用户校园网 + 已扫码 jaccount 时跑）2026-05-18**
```

- [ ] **Step 13.4: tasks/todo.md 同步**

打开 `tasks/todo.md`，加新章节：

```markdown
### 2026-05-18 T4 weixin path fallback ✅
14 task 全 done：Envelope.meta / via.rs / models 扩展 / weixin/{money, balance_parse, history_parse, client, mod} / data 转换 / handlers --via / cli --via / 健康检查 / 文档同步。

待真机 CP：
- CP-WX-BAL：登录 + 校园网内 `sjtu card balance --via weixin` 跑通
- CP-WX-HIST：`sjtu card history --days 30 --via weixin` 跑通
- CP-WX-STALE：模拟 30 分钟不活动 → 再次跑 → 应 SubSessionStale → cas refresh → 成功
- OQ-WX-1：实测 `?startdate=YYYY-MM-DD&enddate=YYYY-MM-DD` 是否服务端识别（否则查实际参数名）
- OQ-WX-2：stale 形态实测（redirect URL 是 jalogin 还是 oauth2/authorize）
- OQ-WX-3：HTML selector 稳定性观察
```

- [ ] **Step 13.5: tasks/lessons.md 加经验条目**

打开 `tasks/lessons.md`，追加：

```markdown
### 2026-05-18 — T4 weixin path 双轨 fallback

**Plan deviation from spec**：spec 设计 `transition_balance: Option<Decimal>` 作 weixin 独有字段时，遗漏 OAuth2 path 现有 `trans_balance: Decimal`（非 Option）已经覆盖相同语义。Plan 阶段统一为 OAuth2 现有字段，drop 了 `util::decimal_opt` 模块。

**经验**：spec 写"独有字段"时，先 grep 现有 struct 字段名同义近义，避免 plan 阶段才发现重复。spec 阶段 self-review 应包含「字段唯一性扫描」。

**Envelope.meta 后向兼容设计**：`meta: Option<EnvelopeMeta>` + 内部字段 `Option<String>` + `skip_serializing_if = "Option::is_none"`，使得现有所有子系统（5 个）的 JSON 输出形态 0 变化，新 card 双轨子系统按需填 meta。Agent 解析方应把 meta 视为 optional 字段。

**Cookie struct 注入 reqwest jar 的工程坑**：`crate::cookies::Cookie` 是纯数据 struct 无方法，spec 阶段误以为有 `serialize()` 方法。weixin/client.rs 自卷 `cookie_to_set_str(&Cookie) -> String`（拼 `name=value; Domain=; Path=`）喂 `Jar::add_cookie_str`。`expires` 字段故意不拼 —— Jar 不读它，stale 由 `SubSessionStale` 信号驱动重 CAS。

**`with_cas_refresh` 复用**：weixin path 与 jwc/elec/services 同款 cookie-based 子系统，复用 T8 的 retry helper。stale variant `SubSessionStale("card_weixin")` 由 `detect_stale_or_unexpected` 在响应 URL 落到 `jaccount/jalogin` 或 `oauth2/authorize` 时手动抛。
```

- [ ] **Step 13.6: 一次性 commit 文档**

```powershell
git add README.md SKILL.md CLAUDE.md tasks/todo.md tasks/lessons.md
git commit -m "docs(t4): weixin path fallback 收尾 — README/SKILL/CLAUDE/todo/lessons 同步"
```

---

## Task 14: 总收尾 — CP 真机清单 + 健康检查复跑

**Files:** 无修改

- [ ] **Step 14.1: 复跑健康检查（同 Task 12，确认文档同步未引入回归）**

Run:
```powershell
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo test
```

Expected: 4 项全绿。

- [ ] **Step 14.2: 列 CP 真机清单交给用户（写在终端输出或在 todo.md 里 highlight）**

CP 真机清单（用户跑，CLI 工作流外）：

```text
CP-WX-BAL    sjtu card balance --via weixin                          # 校园网内 + 已 jaccount login
CP-WX-HIST   sjtu card history --days 7 --via weixin
CP-WX-STALE  连续 cmd 间隔 35 min → 触发 stale → auto refresh → 仍 OK
CP-WX-AUTO   sjtu card balance（默认 auto）→ 无 OAuth2 token 应自动 weixin → meta.via=weixin
OQ-WX-1      `sjtu card history --days 30 --via weixin` 抓包看 URL query 参数名实际是 startdate/enddate 还是其它
OQ-WX-2      `sjtu card balance --via weixin`（session 已 stale 时）抓 redirect URL，确认 stale-detect 命中
OQ-WX-3      手动浏览器打开 ecardbalance.php / ecardbill.php 比对当前 HTML 结构 vs fixture
```

- [ ] **Step 14.3: 全部 task done，宣布 plan 完结**

无 commit。用户跑 CP 清单后填回 OQ-WX-1/2/3 结果，可能触发小 fix-up commit。

---

## Self-Review Checklist

**1. Spec coverage**

| Spec 章节 | Plan task |
|---|---|
| §0 决策表 | 全 plan 体现 |
| §1.1 入口契约 redirect chain | Task 8 `detect_stale_or_unexpected` 在 redirect 落 jaccount 时抛 stale |
| §1.2 关键事实（client_id / scope）| weixin path 复用网信中心 OAuth2，CLI 不直接调；Task 8 流程隐含 |
| §2.1 余额 HTML 字段表 | Task 5 `balance_parse.rs` 完整覆盖 |
| §2.2 流水 HTML 字段表 | Task 6 `history_parse.rs` 完整覆盖 |
| §2.3 时间格式归一 +08:00 | Task 6 测试 `datetime_serialized_as_beijing_ms` |
| §3.1 `--via` flag | Task 11 `cli/card.rs` |
| §3.2 路径选择器 auto 行为 | Task 2 `via.rs::select_via` + Task 10 `handlers` 调用 |
| §3.3 Envelope `meta.via` | Task 1 `output.rs::EnvelopeMeta` + Task 10 构造 |
| §4 文件骨架 | Task 9（data.rs）/ Task 11 / Task 13 完全对齐 |
| §5 鉴权层 with_cas_refresh | Task 7（client）+ Task 8（with_cas_refresh 包装）|
| §6.1 Models 共享 struct + 字段 default | Task 3 加 enum + `#[serde(default, skip_serializing_if)]` |
| §6.2 HTML 解析 fallback | Task 5/6 字段缺失 warn + 默认值 |
| §7 写端点红线 | 全 plan 无任何 POST/PUT/DELETE 端点；Task 14 CP 清单也仅 GET |
| §8 Open Questions OQ-WX-1/2/3 | Task 14 留 CP 阶段解 |
| §9 Out-of-scope | OAuth2 path 0 改动（仅 handlers.rs 把现有 cmd_balance body 搬到 inner，逻辑不变）|
| §10 测试策略 | 全 plan TDD + mockito 风格；integration test `#[ignore]` 留 CP |

**2. Placeholder scan**

- [x] 无 "TBD" / "TODO" / "implement later"
- [x] 每个 step 含具体代码或具体命令
- [x] 错误消息均明确（"卡账号"/"校园卡余额" 等字段名都写明）
- [x] 测试代码完整可跑

**3. Type consistency**

- `CardVia` 在 Task 2 定义 / Task 10/11 消费 — 一致
- `ResolvedVia.name()/.source_hint()` 在 Task 2 定义 / Task 10 envelope 构造 — 一致
- `EnvelopeMeta { via, source_hint }` 在 Task 1 / Task 10 — 一致
- `CardLostStatus::{Normal, Lost}` 在 Task 3 / Task 5 / Task 9 — 一致
- `parse_money_zh` 在 Task 4 定义 / Task 5/6 调用 — 一致
- `with_cas_refresh("card_weixin", url, |session| async { ... })` Task 8 — 与 T8 retry.rs:37 signature 对齐
- `Transaction.date_time_ms: i64` Task 6 / Task 9 转 ISO8601 — 一致

⚠️ 一处需 Task 9 现场确认：`TransactionItem::from_oauth2_transaction` 方法名 — Step 9.1 必须先 Read 现有 `data.rs` 确认；如不同则就地调整。Plan 已显式提示。

---

## Execution Notes

- **执行模式**：subagent-driven 执行（superpowers:subagent-driven-development），fresh subagent per task + spec/code-quality two-stage review。
- **Task 11 → Task 10 顺序**：handlers 改造（Task 10）需要 cli/card.rs（Task 11）的 `via: CardVia` 参数还没接通，所以 Task 11 cargo check 会失败 — **必须 Task 10 + Task 11 顺序连做，不能在 Task 10 之后跑 cargo test**。Step 10.5 已注明此前置依赖。
- **CP 真机 task 不在本 plan**：因为需要用户在校园网内 + 已扫码登录的物理环境。Plan 结束 = 全部代码就绪 + 单元 100% + 文档同步。CP 触发新一轮跑 / OQ 回填，回填后可能产生 small fix-up commit。
