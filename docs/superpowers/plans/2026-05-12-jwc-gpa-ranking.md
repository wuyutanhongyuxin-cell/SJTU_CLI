# T2 jwc GPA + Rankings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 `sjtu jwc gpa` 命令补完排名双轨输出（保留 server `"X/Y"` 字符串 + 附加 `RankPair` parsed），并新增 `sjtu jwc gpa-by-semester` 子命令实现多学期循环聚合查询。

**Architecture:** 复用已实装的 N309131 两阶段 SP（`apps/jwc/api/gpa.rs`），新增 `RankPair` 解析 helper 在 model 层，commands 层增加 `gpa_handlers.rs` 承载多学期客户端循环（600ms throttle + fail-soft 双数组）；`commands/jwc/data.rs` 已撞 200 行硬限，本计划顺带拆出 `data/gpa.rs`。

**Tech Stack:** Rust 2021 / clap 4 derive / reqwest / tokio / serde / chrono / mockito（测试） —— 全部已有依赖，零新增。

**Spec:** `docs/superpowers/specs/2026-05-12-jwc-gpa-ranking-design.md`

---

## File Structure Map

| 操作 | 路径 | 责任 | 行数变化 |
|------|------|------|---------|
| Modify | `src/apps/jwc/models/gpa.rs` | +`RankPair` struct +`parse_rank` fn +2 字段 +`impl Gpa::fill_parsed` | 60 → ~135 |
| Modify | `src/apps/jwc/tests_parse.rs` | +6 `parse_rank` 单测 case | 157 → ~210 |
| Split | `src/commands/jwc/data.rs` → `data/mod.rs` + `data/gpa.rs` | 拆 GpaData + 新增 4 个 struct 移到 gpa.rs | 200 → ~135 (mod) + ~95 (gpa) |
| Modify | `src/commands/jwc/handlers.rs` | cmd_gpa 收 envelope 后 fill_parsed 透传 | 125 → ~130 |
| Create | `src/commands/jwc/gpa_handlers.rs` | `cmd_gpa_by_semester` + `enumerate_semesters` + 循环 | 0 → ~130 |
| Modify | `src/commands/jwc/mod.rs` | +`pub use gpa_handlers::cmd_gpa_by_semester` | 17 → ~20 |
| Modify | `src/cli/jwc/mod.rs` | +`JwcSub::GpaBySemester` variant + dispatch arm | 165 → ~195 |
| Create | `tests/fixtures/jwc/n309131_step1_success.txt` | T0 真机脱敏裸字符串 fixture | 0 → 1 |
| Create | `tests/fixtures/jwc/n309131_step2_hxkc_njzy.json` | T0 真机脱敏 step2 envelope fixture | 0 → ~30 |
| Modify | `src/apps/jwc/tests_parse.rs` (Task 7) | +3 集成测试 case（mockito 两阶段串联 + fixture include） | ~210 → ~280 |

---

## Task 1: T0 Fixture Capture — N309131 真机抓取与脱敏

**⚠️ MAIN SESSION ONLY** — subagent 没有 SJTU sub_session / 没有浏览器，必须由主对话亲跑。

**Files:**
- Create: `tests/fixtures/jwc/n309131_step1_success.txt`
- Create: `tests/fixtures/jwc/n309131_step2_hxkc_njzy.json`

- [ ] **Step 1: 确认 jwc CAS sub_session 已建立**

Run:
```powershell
target\release\sjtu.exe jwc grades --xnm 2025 --xqm 12 --limit 5
```
Expected: 任意非空成绩列表（验证 jwc CAS 通）；如失败先 `sjtu login` 重抓主 session 再跑。

- [ ] **Step 2: 跑 N309131 默认组合（hxkc + njzy + 累计）**

Run:
```powershell
target\release\sjtu.exe --json jwc gpa --scope hxkc --rank njzy
```
Expected: stdout 是 JSON Envelope，data.items[0] 含 gpa/gpapm/xjf/xjfpm/czsj 等字段；data.returned >= 1。
（注：`--json` / `--yaml` 是全局 flag，必须置于子命令 `jwc` 之前，clap 不接受 `-o`。）

- [ ] **Step 3: 抓取 step1 与 step2 的原始响应**

观察上一步的 tracing 日志（默认 INFO 级别看不到完整 body）。要拿原始 body 用 `RUST_LOG=debug` 重跑：

Run:
```powershell
$env:RUST_LOG="debug"; target\release\sjtu.exe --json jwc gpa --scope hxkc --rank njzy 2>&1 | Select-String -Pattern "N309131 gpa step"
```

Expected: 看到 `N309131 gpa step1` 与 `step2` 两个 debug 行。

如 debug 日志不含完整 body，改用 chrome-devtools MCP 走浏览器抓：导航到 i.sjtu.edu.cn 教务系统 → 进入 GPA / 学积分查询页面 → 用 `mcp__chrome-devtools__list_network_requests` 找 `gpapmtj_tjGpapmtj.html` + `gpapmtj_cxGpaxjfcxIndex.html` 两个请求 → `get_network_request` 拉 body。

- [ ] **Step 4: 写 step1 fixture（裸字符串）**

```
"统计成功！"
```

文件内容就是上面这一行（**包含** 双引号，这是 JSON 字符串字面量）。

Write: `tests/fixtures/jwc/n309131_step1_success.txt`

- [ ] **Step 5: 写 step2 fixture（脱敏 envelope）**

把真机 step2 的 body 复制到剪贴板，按以下规则脱敏后落盘：

**必删字段**（agent 不允许接触身份信息）：
- `xh` / `xm` / `xh_id` / `bj` / `jgmc` / `zymc` / `njmc` / `bjgmc` / `bjgms`

**保留字段**：
- `gpa` / `gpapm` / `xjf` / `xjfpm` / `zf` / `ms` / `zxf` / `hdxf` / `bjgxf` / `tgl` / `kcfw` / `czsj`
- envelope 外层 `currentPage` / `pageSize` / `totalResult` / `totalPage` / `items`

参考模板（用真机值替换）：
```json
{
  "currentPage": 1,
  "pageSize": 50,
  "totalResult": "1",
  "totalPage": 1,
  "items": [
    {
      "gpa": "3.85",
      "gpapm": "3/120",
      "xjf": "88.5",
      "xjfpm": "5/120",
      "zf": "1234",
      "ms": "14",
      "zxf": "40.0",
      "hdxf": "40.0",
      "bjgxf": "0",
      "tgl": "100%",
      "kcfw": "hxkc",
      "czsj": "2026-04-30 12:34:56"
    }
  ]
}
```

Write: `tests/fixtures/jwc/n309131_step2_hxkc_njzy.json`

- [ ] **Step 6: 验 fixture 反序列化通**

Run:
```powershell
cargo test --quiet -p sjtu-cli parse_gpa_envelope_step2_items
```
Expected: PASS（已有的 inline 测试通；fixture 文件不影响此 case，但确认 cargo 编译通过没引入语法错误）。

- [ ] **Step 7: 记录 OQ1 调研结果（实地试未到统计时间的学期）**

Run:
```powershell
target\release\sjtu.exe --json jwc gpa --scope hxkc --rank njzy --from 203003 --to 203003
```
Expected: 观察 step1 返什么。三种可能：
1. `"统计成功！"` 但 step2 items 空（最常见）
2. step1 直接报错（非"统计成功！"字符串）
3. server 报 5xx

把实际行为记到本步骤下方（如果与默认假设一致，写"已确认 case 1"）：

> **实地观察记录**（2026-05-12）：已确认 case 1 — step1 成功（envelope ok=true）但 step2 items 空（total_result=0, returned=0, exit code 0）。server 未给"未到统计时间"独立报错，client 通过 `items.is_empty()` fail-soft 识别（已落到 Task 5 cmd_gpa_by_semester 的 `Ok(env) if env.items.is_empty()` 分支）。

- [ ] **Step 8: Commit fixture**

```powershell
git add tests/fixtures/jwc/n309131_step1_success.txt tests/fixtures/jwc/n309131_step2_hxkc_njzy.json
git commit -m "test(jwc): add N309131 GPA fixture (step1 success + step2 hxkc/njzy desensitized)"
```

---

## Task 2: `RankPair` + `parse_rank` + 6 unit tests

**Files:**
- Modify: `src/apps/jwc/models/gpa.rs` (60 → ~115)
- Modify: `src/apps/jwc/tests_parse.rs` (157 → ~215)

- [ ] **Step 1: Write the failing tests**

在 `src/apps/jwc/tests_parse.rs` 末尾追加：

```rust
use super::models::gpa::{parse_rank, RankPair};

#[test]
fn parse_rank_normal_3_over_120() {
    let r = parse_rank("3/120").unwrap();
    assert_eq!(r.rank, 3);
    assert_eq!(r.total, 120);
    let p = r.percentile.unwrap();
    assert!((p - 2.5).abs() < 1e-6, "percentile={p}");
}

#[test]
fn parse_rank_total_zero_keeps_pair_drops_percentile() {
    let r = parse_rank("0/0").unwrap();
    assert_eq!(r.rank, 0);
    assert_eq!(r.total, 0);
    assert!(r.percentile.is_none(), "total=0 时 percentile 必须 None");
}

#[test]
fn parse_rank_empty_returns_none() {
    assert!(parse_rank("").is_none());
    assert!(parse_rank("   ").is_none());
}

#[test]
fn parse_rank_no_slash_returns_none() {
    assert!(parse_rank("3").is_none());
    assert!(parse_rank("abc").is_none());
    assert!(parse_rank("3-120").is_none());
}

#[test]
fn parse_rank_rank_greater_than_total_drops_percentile() {
    let r = parse_rank("200/120").unwrap();
    assert_eq!(r.rank, 200);
    assert_eq!(r.total, 120);
    assert!(r.percentile.is_none(), "rank>total 时 percentile 必须 None");
}

#[test]
fn parse_rank_tolerates_whitespace() {
    let r = parse_rank("  3 / 120  ").unwrap();
    assert_eq!(r.rank, 3);
    assert_eq!(r.total, 120);
    assert_eq!(r.percentile, Some(2.5));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```powershell
cargo test --quiet -p sjtu-cli parse_rank 2>&1 | Select-Object -First 30
```
Expected: 编译失败，错误类型 `unresolved import: super::models::gpa::parse_rank` 或 `cannot find function parse_rank` —— 因为 parse_rank 还没写。

- [ ] **Step 3: Implement `RankPair` + `parse_rank` in models/gpa.rs**

在 `src/apps/jwc/models/gpa.rs` 末尾追加（在原 `Gpa` struct 定义之后）：

```rust
/// 排名字符串 `"X/Y"` 的结构化表达。仅 client 端使用（server 不返回）。
///
/// 设计点：
/// - `rank` / `total` 保留原始整数（不做单位转换）。
/// - `percentile` = rank / total × 100。fail-soft 边界：
///   - `total == 0` → `None`（除零）
///   - `rank > total` → `None`（数据异常，但保留 rank/total 供审计）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RankPair {
    pub rank: u32,
    pub total: u32,
    pub percentile: Option<f64>,
}

/// fail-soft 解析 ZF 排名字符串 `"X/Y"`。
///
/// 容错策略：
/// - 空字符串 / 全空白 → `None`
/// - 无斜杠 / 两侧不是 u32 → `None`
/// - `total == 0` 或 `rank > total` → `Some(RankPair { rank, total, percentile: None })`
pub fn parse_rank(s: &str) -> Option<RankPair> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (l, r) = trimmed.split_once('/')?;
    let rank: u32 = l.trim().parse().ok()?;
    let total: u32 = r.trim().parse().ok()?;
    let percentile = if total == 0 || rank > total {
        None
    } else {
        Some((rank as f64 / total as f64) * 100.0)
    };
    Some(RankPair { rank, total, percentile })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:
```powershell
cargo test --quiet -p sjtu-cli parse_rank
```
Expected:
```
test apps::jwc::tests_parse::parse_rank_normal_3_over_120 ... ok
test apps::jwc::tests_parse::parse_rank_total_zero_keeps_pair_drops_percentile ... ok
test apps::jwc::tests_parse::parse_rank_empty_returns_none ... ok
test apps::jwc::tests_parse::parse_rank_no_slash_returns_none ... ok
test apps::jwc::tests_parse::parse_rank_rank_greater_than_total_drops_percentile ... ok
test apps::jwc::tests_parse::parse_rank_tolerates_whitespace ... ok
```
6 passed.

- [ ] **Step 5: fmt + clippy + 行数 check**

Run:
```powershell
cargo fmt; cargo clippy --quiet -p sjtu-cli --all-targets -- -D warnings; (Get-Content src\apps\jwc\models\gpa.rs | Measure-Object -Line).Lines
```
Expected: fmt 无 diff；clippy 0 warning；行数 < 200。

- [ ] **Step 6: Commit**

```powershell
git add src/apps/jwc/models/gpa.rs src/apps/jwc/tests_parse.rs
git commit -m "feat(jwc): add RankPair + parse_rank for GPA ranking dual-track output"
```

---

## Task 3: `Gpa::fill_parsed` + `cmd_gpa` 透传

**Files:**
- Modify: `src/apps/jwc/models/gpa.rs` (~115 → ~135)
- Modify: `src/commands/jwc/handlers.rs` (125 → ~130)

- [ ] **Step 1: 在 models/gpa.rs 加 parsed 字段 + impl Gpa::fill_parsed**

把 `pub struct Gpa { ... }` 末尾两个字段追加，并在 struct 后加 impl 块：

```rust
// 在 Gpa struct 内部、czsj 字段之后追加：
    /// 客户端从 `gpapm` 解析的结构化排名（server 不返回，cmd 层 fill_parsed 填）。
    #[serde(default, skip_deserializing)]
    pub gpapm_parsed: Option<RankPair>,
    /// 客户端从 `xjfpm` 解析的结构化排名（server 不返回，cmd 层 fill_parsed 填）。
    #[serde(default, skip_deserializing)]
    pub xjfpm_parsed: Option<RankPair>,
}

impl Gpa {
    /// 从原始 `gpapm` / `xjfpm` 字符串字段解析填充 `*_parsed`。
    /// fail-soft：解析失败的字段保持 `None`，不抛错。
    pub fn fill_parsed(&mut self) {
        self.gpapm_parsed = self.gpapm.as_deref().and_then(parse_rank);
        self.xjfpm_parsed = self.xjfpm.as_deref().and_then(parse_rank);
    }
}
```

- [ ] **Step 2: 加一条 fill_parsed 集成测试到 tests_parse.rs**

在 tests_parse.rs 末尾追加：

```rust
#[test]
fn gpa_fill_parsed_populates_both_from_strings() {
    let mut g = Gpa {
        gpapm: Some("3/120".into()),
        xjfpm: Some("5/120".into()),
        ..Default::default()
    };
    g.fill_parsed();
    assert_eq!(g.gpapm_parsed.as_ref().unwrap().rank, 3);
    assert_eq!(g.xjfpm_parsed.as_ref().unwrap().rank, 5);
}

#[test]
fn gpa_fill_parsed_keeps_none_when_strings_invalid() {
    let mut g = Gpa {
        gpapm: Some("".into()),
        xjfpm: None,
        ..Default::default()
    };
    g.fill_parsed();
    assert!(g.gpapm_parsed.is_none());
    assert!(g.xjfpm_parsed.is_none());
}
```

- [ ] **Step 3: Run tests, verify fail (Gpa struct does not yet have parsed fields)**

Run:
```powershell
cargo test --quiet -p sjtu-cli gpa_fill_parsed 2>&1 | Select-Object -First 20
```
Expected: 编译失败，错误如 `no field gpapm_parsed on Gpa` —— 这是预期的（field 已加但 fill_parsed 未加完整？根据 Step 1 已经加完，应该编译通过并测试 PASS。如果 Step 1 已加 fill_parsed 则这里直接 PASS）。

**注意**：Step 1 与 Step 2 实际上是同 commit，所以编译应该成功；这一步主要确认 PASS。

- [ ] **Step 4: 改 cmd_gpa 透传 fill_parsed**

Modify `src/commands/jwc/handlers.rs` 的 `cmd_gpa` 函数：

```rust
pub async fn cmd_gpa(
    scope: GpaScope,
    rank: GpaRank,
    qs_xnxq: Option<String>,
    zz_xnxq: Option<String>,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let client = Client::connect().await?;
    let mut env_resp = client
        .gpa(scope, rank, qs_xnxq.as_deref(), zz_xnxq.as_deref())
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

注意：原 `let env_resp` 改为 `let mut env_resp`，加 4 行 for loop。

- [ ] **Step 5: cargo check + 跑所有 gpa 相关测试**

Run:
```powershell
cargo test --quiet -p sjtu-cli gpa
```
Expected: 所有 gpa 相关测试 PASS（含已有 `parse_gpa_envelope_step2_items` 与新加 2 个 fill_parsed case）。

- [ ] **Step 6: fmt + clippy + 行数 check**

Run:
```powershell
cargo fmt; cargo clippy --quiet -p sjtu-cli --all-targets -- -D warnings; (Get-Content src\apps\jwc\models\gpa.rs, src\commands\jwc\handlers.rs | Measure-Object -Line).Lines
```
Expected: clippy 0 warning；models/gpa.rs ~135 < 200；handlers.rs ~130 < 200。

- [ ] **Step 7: Commit**

```powershell
git add src/apps/jwc/models/gpa.rs src/apps/jwc/tests_parse.rs src/commands/jwc/handlers.rs
git commit -m "feat(jwc): wire Gpa::fill_parsed into cmd_gpa for dual-track ranking output"
```

---

## Task 4: 拆分 `commands/jwc/data.rs` → `data/{mod.rs, gpa.rs}`

**Files:**
- Create: `src/commands/jwc/data/mod.rs` (~135 行)
- Create: `src/commands/jwc/data/gpa.rs` (~95 行 含新增 4 struct)
- Delete: `src/commands/jwc/data.rs` (Rust 拆分时旧 single-file 被 mod 目录替代，git mv 不强制)

- [ ] **Step 1: 确认现状**

Run:
```powershell
(Get-Content src\commands\jwc\data.rs | Measure-Object -Line).Lines
```
Expected: 200（已触底，必须拆）。

- [ ] **Step 2: 创建 data/mod.rs（含原 GradesData / ScheduleData / ExamsData / TodayData / WeekData / NextData / TodayItem / WeekItem / NextItem 与原 inline tests）**

把 `src/commands/jwc/data.rs` 整文件内容复制为 `src/commands/jwc/data/mod.rs`，然后做两处改：
1. 顶部 doc comment 改为 "拆分入口；GPA 相关 struct 见 `gpa.rs`"
2. **移除** `GpaData` struct 定义（连同其上方一整段 doc comment）—— GpaData 会移到 gpa.rs
3. 顶部 `use` 区加：`pub use gpa::{GpaBySemesterData, GpaData, SemesterFailure, SemesterGpa, SemesterKey};`
4. 顶部加 `mod gpa;`

具体新文件 `src/commands/jwc/data/mod.rs` 头部应该是：

```rust
//! `sjtu jwc <sub>` 命令暴露给 Envelope 的数据形状。
//!
//! 拆分入口；GPA 相关 struct（GpaData / GpaBySemesterData / SemesterGpa / SemesterFailure /
//! SemesterKey）见 `gpa.rs`。

#![allow(dead_code)]

use serde::Serialize;
use serde_json::Value;

use crate::apps::jwc::{Exam, Grade, KbItem, RqAzc};

mod gpa;
pub(super) use gpa::{GpaBySemesterData, GpaData, SemesterFailure, SemesterGpa, SemesterKey};

// ... 原 GradesData / ScheduleData / ExamsData / TodayData / WeekData / NextData
//      / TodayItem / WeekItem / NextItem / 原 inline tests 都保留 ...
```

**关键**：原 `use crate::apps::jwc::{Exam, Gpa, Grade, KbItem, RqAzc};` 里 `Gpa` 删除（因为 GpaData 不在本文件了）。

- [ ] **Step 3: 创建 data/gpa.rs**

```rust
//! GPA 相关命令的 Envelope data 形状。
//!
//! - `GpaData` ── `sjtu jwc gpa` 单学期/累计查询
//! - `GpaBySemesterData` ── `sjtu jwc gpa-by-semester` 多学期循环
//!
//! 设计：成功学期与失败学期分双数组（fail-soft），agent 拿 `failed.len()`
//! 即知本次有多少学期被跳过（"未到统计时间" / "网络挂" / "items 空"）。

#![allow(dead_code)]

use serde::Serialize;
use serde_json::Value;

use crate::apps::jwc::models::gpa::{Gpa, RankPair};

/// `sjtu jwc gpa` 的 data 形状。`items[0]` 通常即当前学生。
#[derive(Debug, Serialize)]
pub(in crate::commands::jwc) struct GpaData {
    /// 查询入参回显。
    pub scope: &'static str, // hxkc / qbkc
    pub rank: &'static str,  // njzy / nj / bj
    pub qs_xnxq: Option<String>,
    pub zz_xnxq: Option<String>,

    pub total_result: Option<Value>,
    pub returned: usize,
    pub items: Vec<Gpa>,
}

/// `sjtu jwc gpa-by-semester` 的 data 形状。
#[derive(Debug, Serialize)]
pub(in crate::commands::jwc) struct GpaBySemesterData {
    pub scope: &'static str,
    pub rank: &'static str,
    pub xnm_from: u32,
    pub xnm_to: u32,
    /// 请求过的全部 (xnm, xqm) 组合（含落空的）。
    pub requested: Vec<SemesterKey>,
    /// 成功拿到 GPA 的学期，含 parsed RankPair。
    pub succeeded: Vec<SemesterGpa>,
    /// 失败学期：原因含 "items 空" / "未到统计时间" / 原始 err 文案。
    pub failed: Vec<SemesterFailure>,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::jwc) struct SemesterKey {
    pub xnm: String,
    pub xqm: String,
}

/// 多学期场景每条成功记录。比单学期 GpaData.items[0] 少 7 个细节字段
/// （zf/bjgmc/bjgms/hdxf/bjgxf/tgl/kcfw），核心 GPA/排名/学分齐全。
#[derive(Debug, Serialize)]
pub(in crate::commands::jwc) struct SemesterGpa {
    pub xnm: String,
    pub xqm: String,
    pub gpa: Option<String>,
    pub gpapm: Option<String>,
    pub gpapm_parsed: Option<RankPair>,
    pub xjf: Option<String>,
    pub xjfpm: Option<String>,
    pub xjfpm_parsed: Option<RankPair>,
    pub ms: Option<String>,
    pub zxf: Option<String>,
    pub czsj: Option<String>,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::jwc) struct SemesterFailure {
    pub xnm: String,
    pub xqm: String,
    pub reason: String,
}

impl From<&Gpa> for SemesterGpa {
    /// 从 Gpa 截选 11 字段构造。要求调用方已调过 g.fill_parsed()。
    fn from(g: &Gpa) -> Self {
        Self {
            xnm: String::new(), // 由调用方填
            xqm: String::new(), // 由调用方填
            gpa: g.gpa.clone(),
            gpapm: g.gpapm.clone(),
            gpapm_parsed: g.gpapm_parsed.clone(),
            xjf: g.xjf.clone(),
            xjfpm: g.xjfpm.clone(),
            xjfpm_parsed: g.xjfpm_parsed.clone(),
            ms: g.ms.clone(),
            zxf: g.zxf.clone(),
            czsj: g.czsj.clone(),
        }
    }
}
```

- [ ] **Step 4: 删旧 data.rs**

Run:
```powershell
Remove-Item src\commands\jwc\data.rs
```

- [ ] **Step 5: cargo check 验拆分通**

Run:
```powershell
cargo check --quiet -p sjtu-cli
```
Expected: 零 error，零 warning。如有 `unused import` warning 在 mod.rs，逐条修。

- [ ] **Step 6: cargo test 跑全量验拆分不破坏**

Run:
```powershell
cargo test --quiet -p sjtu-cli
```
Expected: 全 PASS（含 today_data_serializes_with_all_fields / week_data_omits_hint_when_none / next_data_includes_fetched_weeks_and_limit 三个 inline test 应该都跟着移到 mod.rs 一起）。

- [ ] **Step 7: fmt + clippy + 行数 check**

Run:
```powershell
cargo fmt; cargo clippy --quiet -p sjtu-cli --all-targets -- -D warnings; (Get-Content src\commands\jwc\data\mod.rs, src\commands\jwc\data\gpa.rs | Measure-Object -Line).Lines
```
Expected: clippy 0 warning；mod.rs ~135 < 200；gpa.rs ~95 < 200。

- [ ] **Step 8: Commit**

```powershell
git add src/commands/jwc/data src/commands/jwc/data.rs
git commit -m "refactor(jwc): split commands/jwc/data.rs into data/{mod,gpa}.rs (200-line hard limit)"
```

注意：删除旧文件用 `git add` 也会被识别为 deletion；如果 status 显示 `D data.rs`，确保 stage。

---

## Task 5: 新建 `gpa_handlers.rs` 实装 `cmd_gpa_by_semester`

**Files:**
- Create: `src/commands/jwc/gpa_handlers.rs` (~130 行)
- Modify: `src/commands/jwc/mod.rs` (17 → ~21)

- [ ] **Step 1: 写一个 mockito 集成测试 stub（确认接口形态）**

由于本任务的实装主要是循环 + IO 编排，单元测会过度耦合 mockito。改在 Task 7 集中加 mockito 集成测；这里只验**编译时签名**：

新建 `src/commands/jwc/gpa_handlers.rs`，先放 stub：

```rust
//! `sjtu jwc gpa-by-semester` ── 多学期 GPA 客户端循环聚合。
//!
//! 设计：枚举 (xnm, xqm) 组合，按 600ms throttle 串行调 N309131 两阶段 SP；
//! fail-soft 把"网络挂 / step1 拒绝 / items 空"三类失败都装进 `failed` 数组，
//! 整体 exit code 仍为 0（agent 通过 `failed.len()` 自决是否重试）。

use std::time::Duration;

use anyhow::Result;
use chrono::Datelike;

use crate::apps::jwc::{Client, GpaRank, GpaScope};
use crate::output::{render, Envelope, OutputFormat};

use super::data::{GpaBySemesterData, SemesterFailure, SemesterGpa, SemesterKey};

/// 与 T1 schedule 8-周循环对齐的节流（毫秒）。
const SEMESTER_QUERY_THROTTLE_MS: u64 = 600;
/// xqm 枚举：3=秋季 / 12=春季 / 16=夏季。
const XQM_LIST: &[&str] = &["3", "12", "16"];

/// `sjtu jwc gpa-by-semester [--scope] [--rank] [--xnm-from] [--xnm-to]`。
///
/// 默认 `xnm-from` = 当年 - 3（覆盖 4 年制本科），`xnm-to` = 当年。
/// 非 4 年学制需手给。
pub async fn cmd_gpa_by_semester(
    scope: GpaScope,
    rank: GpaRank,
    xnm_from: Option<u32>,
    xnm_to: Option<u32>,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let (from, to) = resolve_xnm_range(xnm_from, xnm_to);
    let requested = enumerate_semesters(from, to);

    let client = Client::connect().await?;

    let mut succeeded: Vec<SemesterGpa> = Vec::new();
    let mut failed: Vec<SemesterFailure> = Vec::new();

    for key in &requested {
        let xnxq = format!("{}{}", key.xnm, key.xqm);
        let res = client
            .gpa(scope, rank, Some(&xnxq), Some(&xnxq))
            .await;
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
            Err(e) => failed.push(SemesterFailure {
                xnm: key.xnm.clone(),
                xqm: key.xqm.clone(),
                reason: format!("{e:#}"),
            }),
        }
    }

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

fn resolve_xnm_range(xnm_from: Option<u32>, xnm_to: Option<u32>) -> (u32, u32) {
    let now_year = chrono::Local::now().year() as u32;
    let to = xnm_to.unwrap_or(now_year);
    let from = xnm_from.unwrap_or(now_year.saturating_sub(3));
    (from, to)
}

fn enumerate_semesters(xnm_from: u32, xnm_to: u32) -> Vec<SemesterKey> {
    let mut out = Vec::with_capacity(((xnm_to - xnm_from + 1) * 3) as usize);
    for xnm in xnm_from..=xnm_to {
        for xqm in XQM_LIST {
            out.push(SemesterKey {
                xnm: xnm.to_string(),
                xqm: (*xqm).to_string(),
            });
        }
    }
    out
}

fn scope_str(s: GpaScope) -> &'static str {
    match s {
        GpaScope::HxKc => "hxkc",
        GpaScope::QbKc => "qbkc",
    }
}

fn rank_str(r: GpaRank) -> &'static str {
    match r {
        GpaRank::NjZy => "njzy",
        GpaRank::Nj => "nj",
        GpaRank::Bj => "bj",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerate_semesters_4_years_yields_12() {
        let xs = enumerate_semesters(2023, 2026);
        assert_eq!(xs.len(), 12);
        assert_eq!(xs[0].xnm, "2023");
        assert_eq!(xs[0].xqm, "3");
        assert_eq!(xs[11].xnm, "2026");
        assert_eq!(xs[11].xqm, "16");
    }

    #[test]
    fn resolve_xnm_range_defaults_to_4_year_window() {
        let now = chrono::Local::now().year() as u32;
        let (from, to) = resolve_xnm_range(None, None);
        assert_eq!(to, now);
        assert_eq!(from, now - 3);
    }

    #[test]
    fn resolve_xnm_range_respects_explicit() {
        let (from, to) = resolve_xnm_range(Some(2020), Some(2024));
        assert_eq!(from, 2020);
        assert_eq!(to, 2024);
    }
}
```

- [ ] **Step 2: 在 mod.rs 加 `mod` + `pub use`**

Modify `src/commands/jwc/mod.rs`：

```rust
//! `sjtu jwc <sub>` 子命令实现。
//!
//! 模块组织（仿 jwbmessage 的拆分；MVP 阶段尚不需 read/write 二分）：
//! - `handlers.rs`：cmd_grades / cmd_schedule / cmd_gpa / cmd_exams
//! - `gpa_handlers.rs`：cmd_gpa_by_semester（多学期循环聚合）
//! - `data/`：Envelope 里承载的 Data struct（mod.rs + gpa.rs）
//!
//! 端点契约见 tasks/isjtu_investigation.md。

mod data;
mod gpa_handlers;
mod handlers;
mod schedule_handlers;
mod schedule_helpers;
mod schedule_next;

pub use gpa_handlers::cmd_gpa_by_semester;
pub use handlers::{cmd_exams, cmd_gpa, cmd_grades, cmd_schedule};
pub use schedule_handlers::{cmd_today, cmd_week};
pub use schedule_next::cmd_next;
```

- [ ] **Step 3: cargo check + 单元测试**

Run:
```powershell
cargo check --quiet -p sjtu-cli; cargo test --quiet -p sjtu-cli gpa_handlers
```
Expected: 3 个 enumerate / resolve_xnm_range 单测 PASS。

- [ ] **Step 4: fmt + clippy + 行数 check**

Run:
```powershell
cargo fmt; cargo clippy --quiet -p sjtu-cli --all-targets -- -D warnings; (Get-Content src\commands\jwc\gpa_handlers.rs, src\commands\jwc\mod.rs | Measure-Object -Line).Lines
```
Expected: clippy 0 warning；gpa_handlers.rs ~130 < 200；mod.rs ~21 < 200。

- [ ] **Step 5: Commit**

```powershell
git add src/commands/jwc/gpa_handlers.rs src/commands/jwc/mod.rs
git commit -m "feat(jwc): add cmd_gpa_by_semester multi-semester GPA loop (600ms throttle, fail-soft)"
```

---

## Task 6: CLI `GpaBySemester` variant + dispatch

**Files:**
- Modify: `src/cli/jwc/mod.rs` (165 → ~195)

- [ ] **Step 1: 加 variant 到 JwcSub enum**

在 `src/cli/jwc/mod.rs` 的 `JwcSub` enum 内，紧跟 `Gpa { ... }` variant 后加：

```rust
    /// 多学期 GPA 对比：客户端循环 N×3 学期 N309131，聚合输出（含 parsed 排名）。
    GpaBySemester {
        /// 课程范围：`hxkc` 核心课 / `qbkc` 全部课。
        #[arg(long, value_enum, default_value_t = GpaScopeArg::Hxkc)]
        scope: GpaScopeArg,

        /// 排名范围：`njzy` 年级专业 / `nj` 年级 / `bj` 班级。
        #[arg(long, value_enum, default_value_t = GpaRankArg::Njzy)]
        rank: GpaRankArg,

        /// 起始学年 4 位（默认当年 - 3，覆盖 4 年制本科；非 4 年学制需手给）。
        #[arg(long)]
        xnm_from: Option<u32>,

        /// 截止学年 4 位（默认当年）。
        #[arg(long)]
        xnm_to: Option<u32>,
    },
```

- [ ] **Step 2: 在 dispatch 函数加 arm**

在 `pub async fn dispatch` 的 match 块内，紧跟 `JwcSub::Gpa { ... } => ...` 行后追加：

```rust
        JwcSub::GpaBySemester {
            scope,
            rank,
            xnm_from,
            xnm_to,
        } => jwc_cmds::cmd_gpa_by_semester(scope.into(), rank.into(), xnm_from, xnm_to, fmt).await,
```

- [ ] **Step 3: cargo check 验 clap 编译通**

Run:
```powershell
cargo check --quiet -p sjtu-cli
```
Expected: 零 error。

- [ ] **Step 4: cargo build + 跑 --help 验 CLI 形态**

Run:
```powershell
cargo build --quiet --release; target\release\sjtu.exe jwc gpa-by-semester --help
```
Expected: 输出包含 `--scope`, `--rank`, `--xnm-from`, `--xnm-to` 四个 flag（注意 clap 把 `xnm_from` 自动转为 `--xnm-from`）。

- [ ] **Step 5: fmt + clippy + 行数 check**

Run:
```powershell
cargo fmt; cargo clippy --quiet -p sjtu-cli --all-targets -- -D warnings; (Get-Content src\cli\jwc\mod.rs | Measure-Object -Line).Lines
```
Expected: clippy 0 warning；cli/jwc/mod.rs ~195 < 200（边缘，但安全）。

- [ ] **Step 6: Commit**

```powershell
git add src/cli/jwc/mod.rs
git commit -m "feat(jwc): expose 'sjtu jwc gpa-by-semester' CLI subcommand"
```

---

## Task 7: mockito 集成测试（两阶段串联 + fixture include）

**Files:**
- Modify: `src/apps/jwc/tests_parse.rs` (~215 → ~280)

- [ ] **Step 1: 写 3 个失败的集成测试**

在 `src/apps/jwc/tests_parse.rs` 末尾追加：

```rust
// === T2 集成测试：N309131 两阶段串联 + fixture 反序列化 ===

const N309131_STEP1_FIXTURE: &str =
    include_str!("../../../tests/fixtures/jwc/n309131_step1_success.txt");

const N309131_STEP2_FIXTURE: &str =
    include_str!("../../../tests/fixtures/jwc/n309131_step2_hxkc_njzy.json");

#[test]
fn fixture_step1_parses_as_success_string() {
    let s: String = serde_json::from_str(N309131_STEP1_FIXTURE).unwrap();
    assert!(s.contains("统计成功"), "fixture 必须含'统计成功'：{s}");
}

#[test]
fn fixture_step2_parses_envelope_with_required_fields() {
    let p: JwcPage<Gpa> = serde_json::from_str(N309131_STEP2_FIXTURE).unwrap();
    assert!(!p.items.is_empty(), "fixture step2 必须含至少 1 条 item");
    let g = &p.items[0];
    assert!(g.gpa.is_some(), "gpa 字段必须保留");
    assert!(g.gpapm.is_some(), "gpapm 字段必须保留");
    assert!(g.kcfw.is_some(), "kcfw 字段必须保留（server 返中文 '核心课程' 而非 hxkc）");
}

#[test]
fn fixture_step2_round_trip_with_fill_parsed() {
    let mut p: JwcPage<Gpa> = serde_json::from_str(N309131_STEP2_FIXTURE).unwrap();
    for g in &mut p.items {
        g.fill_parsed();
    }
    let g = &p.items[0];
    // 双轨断言：原字符串保留 + parsed 填充
    assert!(g.gpapm.is_some());
    assert!(g.gpapm_parsed.is_some(), "fill_parsed 后 gpapm_parsed 必须有值");
    let rp = g.gpapm_parsed.as_ref().unwrap();
    assert!(rp.total > 0, "fixture 排名 total 必须 > 0");
}
```

- [ ] **Step 2: Run tests, verify they pass**

Run:
```powershell
cargo test --quiet -p sjtu-cli fixture_step
```
Expected:
```
test apps::jwc::tests_parse::fixture_step1_parses_as_success_string ... ok
test apps::jwc::tests_parse::fixture_step2_parses_envelope_with_required_fields ... ok
test apps::jwc::tests_parse::fixture_step2_round_trip_with_fill_parsed ... ok
```
3 passed.

注意：这些测试直接断言 fixture 反序列化与 fill_parsed 集成，不走 mockito（fixture 已落盘 + Gpa struct 已经测过反序列化）。完整 mockito 端到端的 client.gpa() 走 N309131 SP 的测试需要修改 jwc Client 的 base URL，本期不做（开放为 T3 后续 task；本期保持 spec §7.2 的最小集成测试集）。

- [ ] **Step 3: fmt + clippy + 行数 check**

Run:
```powershell
cargo fmt; cargo clippy --quiet -p sjtu-cli --all-targets -- -D warnings; (Get-Content src\apps\jwc\tests_parse.rs | Measure-Object -Line).Lines
```
Expected: clippy 0 warning；tests_parse.rs ~280 < 300。

- [ ] **Step 4: Commit**

```powershell
git add src/apps/jwc/tests_parse.rs
git commit -m "test(jwc): add N309131 fixture round-trip + fill_parsed integration tests"
```

---

## Task 8: 真机 smoke matrix (main session)

**⚠️ MAIN SESSION ONLY** — 真机网络依赖。

**Files:** 无 — 只跑命令、观察输出、记录到 lessons.md。

- [ ] **Step 1: cargo build --release**

Run:
```powershell
cargo build --quiet --release
```
Expected: 零 error。

- [ ] **Step 2: 单学期 6 组合（hxkc/qbkc × njzy/nj/bj）**

逐条跑：

```powershell
target\release\sjtu.exe --yaml jwc gpa --scope hxkc --rank njzy
target\release\sjtu.exe --yaml jwc gpa --scope hxkc --rank nj
target\release\sjtu.exe --yaml jwc gpa --scope hxkc --rank bj
target\release\sjtu.exe --yaml jwc gpa --scope qbkc --rank njzy
target\release\sjtu.exe --yaml jwc gpa --scope qbkc --rank nj
target\release\sjtu.exe --yaml jwc gpa --scope qbkc --rank bj
```

每跑一条断言：
- `data.items[0].gpapmParsed` 非 null 且 `total > 0`
- `data.items[0].xjfpmParsed` 非 null
- `data.items[0].czsj` 是合理的近期时间戳（"2026-04" 或 "2026-05"）
- `bj` 组合的 total 远小于 `njzy` / `nj` 的 total

把异常组合记到 Step 5。

- [ ] **Step 3: 多学期 gpa-by-semester smoke**

Run:
```powershell
target\release\sjtu.exe --yaml jwc gpa-by-semester
```

Expected:
- `data.requested.len()` = 12
- `data.succeeded.len()` ≥ 1（至少当前学期）
- `data.failed.len()` ≥ 0（历史未选课学期可能 fail-soft）
- 整体耗时约 7-10s（含 600ms × 12 throttle + 网络 RTT）

- [ ] **Step 4: fail-soft 边界 case**

Run:
```powershell
target\release\sjtu.exe --yaml jwc gpa-by-semester --xnm-from 2030 --xnm-to 2030
```

Expected:
- 全 3 个学期都进 `failed` 数组
- exit code 0（不 panic 不抛异常）
- `failed[*].reason` 含"items 空"或"未到统计时间"

- [ ] **Step 5: 把真机观察记入 lessons.md（与 Task 9 合并 commit）**

观察笔记暂记，下一 task 一起入库。模板：

> N309131 真机记录（2026-05-12）：
> - 6 单学期组合 ✓/✗
> - gpa-by-semester 12 学期 ✓/✗，耗时 X 秒
> - fail-soft 边界（2030 年）✓/✗，reason 文案：`...`
> - OQ1 结论：[填写]

- [ ] **Step 6: 无 commit**（本 task 不改文件；笔记进 Task 9 一起 commit）

---

## Task 9: 文档收尾

**Files:**
- Modify: `tasks/todo.md`
- Modify: `tasks/lessons.md`
- Modify: `README.md`
- Modify: `SKILL.md`

- [ ] **Step 1: tasks/todo.md 勾掉 T2**

打开 `tasks/todo.md`，找到 `T2 GPA 计算 + 学期均分` 行，把 `[ ]` 改为 `[x]`。如该 task 还有 sub-items，全部勾掉。

- [ ] **Step 2: tasks/lessons.md 加 T2 lesson 条目**

在 `tasks/lessons.md` 末尾追加（用 Task 8 真机观察的实际数据填充 [填写] 段）：

```markdown
## 2026-05-12 T2 jwc GPA + 排名双轨

### N309131 两阶段 SP 客户端循环坑

- **step1 必须先发**：跳过 step1 直接 step2 server 会返空 items 而非报错，client 无法识别错误源
- **step1 响应是裸 JSON 字符串**（`"统计成功！"`），不是对象 —— serde_json 反序列化时直接 `from_str::<String>`，**不能** `from_str::<Value>`（拿到的是 Value::String 还要再 .as_str()）
- **12 学期循环 throttle 600ms 安全**（与 T1 schedule 8 周循环对齐），实测整体耗时 ~7-10s
- **fail-soft 三个 case**：网络挂 / step1 拒绝 / items 空 都装进 `failed` 数组，exit code 0
- **xnm-from 默认 "当年-3"**：4 年制本科覆盖率高；非 4 年学制（研究生/留学生）手给 `--xnm-from` 即可，client 不嗅探毕业信息表

### 排名 server-side 给 "X/Y" 字符串 → 双轨 parsed

- 不破坏现有 `gpapm`/`xjfpm` 原字段，**附加** `gpapm_parsed` / `xjfpm_parsed: Option<RankPair>`
- `RankPair.percentile` 在 `total=0` 或 `rank>total` 时为 `None`（fail-soft 而非 panic）
- 用 `#[serde(default, skip_deserializing)]` 让 server 端漂移加字段时反序列化不破，client 端 `Gpa::fill_parsed()` 一次填到位

### data.rs 200 行硬限触底拆分

- `commands/jwc/data.rs` 已 200 行 → 拆出 `data/{mod, gpa}.rs`
- GPA 相关 4 个 struct（GpaData / GpaBySemesterData / SemesterGpa / SemesterFailure / SemesterKey）一起搬到 `data/gpa.rs`
- mod.rs 用 `pub(in crate::commands::jwc) use gpa::{...}` re-export，外层 handler 调用零改动
```

- [ ] **Step 3: README.md 加命令行**

打开 `README.md`，找到现有命令列表（应该已经有 `sjtu jwc gpa`），在其后追加一行：

```
| `sjtu jwc gpa-by-semester` | 多学期 GPA 对比（自动循环 4 年 × 3 学期 N309131） |
```

如果 README 用其他格式（非表格），按现有格式追加。

- [ ] **Step 4: SKILL.md 加 agent 使用说明**

打开 `SKILL.md`，找到 jwc 相关 section，在 `sjtu jwc gpa` 说明后追加：

```markdown
### sjtu jwc gpa-by-semester
多学期 GPA 对比，客户端循环 N309131。

**入参**：
- `--scope hxkc|qbkc`（默认 hxkc）
- `--rank njzy|nj|bj`（默认 njzy）
- `--xnm-from <YYYY>`（默认当年 - 3）
- `--xnm-to <YYYY>`（默认当年）

**输出**：
- `data.succeeded[]`：成功学期，每条含 `gpapm_parsed.rank/total/percentile`
- `data.failed[]`：失败学期（"items 空" / "网络挂" / "未到统计时间"）
- exit code 始终 0；agent 通过 `failed.len()` 判断是否需要重试

**典型耗时**：12 学期 ~7-10 秒（含 600ms × 12 throttle + 网络 RTT）。
```

- [ ] **Step 5: cargo fmt 最终扫一遍 + cargo test 全量**

Run:
```powershell
cargo fmt; cargo test --quiet -p sjtu-cli
```
Expected: 全 PASS（含已有 schedule / grades / gpa 全套）。

- [ ] **Step 6: 行数总检查**

Run:
```powershell
(Get-ChildItem src\apps\jwc\models\gpa.rs, src\apps\jwc\tests_parse.rs, src\commands\jwc\data\mod.rs, src\commands\jwc\data\gpa.rs, src\commands\jwc\handlers.rs, src\commands\jwc\gpa_handlers.rs, src\commands\jwc\mod.rs, src\cli\jwc\mod.rs | ForEach-Object { "{0,-50} {1}" -f $_.Name, (Get-Content $_.FullName | Measure-Object -Line).Lines })
```
Expected: 全部 < 200（tests_parse.rs 测试文件 < 300）。

- [ ] **Step 7: Commit**

```powershell
git add tasks/todo.md tasks/lessons.md README.md SKILL.md
git commit -m "docs(jwc): T2 GPA + ranking dual-track + gpa-by-semester wrap-up"
```

---

## Self-Review

### Spec coverage

- spec §1 (现状) — Task 0 上下文 ✓
- spec §2 Goals G1 (双轨 parsed) — Task 2 + 3 ✓
- spec §2 Goals G2 (gpa-by-semester) — Task 5 + 6 ✓
- spec §2 Goals G3 (fixture + 集成测) — Task 1 + 7 ✓
- spec §2 Goals G4 (真机 smoke 6+1) — Task 8 ✓
- spec §2 Goals G5 (文档) — Task 9 ✓
- spec §4.1 RankPair / parse_rank — Task 2 完整 ✓
- spec §4.2 Gpa.fill_parsed — Task 3 ✓
- spec §4.3 GpaBySemesterData / SemesterGpa / SemesterFailure — Task 4 ✓
- spec §5.1 学期枚举默认 — Task 5 (resolve_xnm_range 单测) ✓
- spec §5.2 600ms throttle + fail-soft — Task 5 ✓
- spec §6 CLI 形态 — Task 6 ✓
- spec §7.1 6 单测 — Task 2 ✓
- spec §7.2 集成测试 — Task 7（注：spec 提到 mockito step1/step2 串联，本期仅做 fixture round-trip；完整 mockito 留 T3+） ⚠️ **降级说明**
- spec §7.3 T0 fixture — Task 1 ✓
- spec §7.4 真机 smoke 6+1 — Task 8 ✓
- spec §8 OQ1 step1 行为 — Task 1 Step 7 记录 ✓
- spec §8 OQ2 风控 — Task 8 Step 3 实测 ✓
- spec §8 OQ3 学制 — Task 9 doc 说明 ✓
- spec §8 OQ4 字段截选 — Task 4 data/gpa.rs 注释 ✓
- spec §9 文件结构 / 行数预算 — Task 4 拆分 + Task 9 Step 6 总检查 ✓

**Gap 标记**：spec §7.2 提到的"完整 mockito 端到端跑通 client.gpa()"在本 plan 降级为"fixture 反序列化 + fill_parsed 集成"。降级理由：jwc Client 当前没有 base URL 注入接口，mockito 端到端需先重构 Client（超本 plan 范围）。如要补齐，开 sub-task "T2.x mockito Client base URL injection"。

### Placeholder scan

- 无 "TBD" / "TODO" / "implement later"。
- Task 1 Step 7 与 Step 5 模板里有 "[填写]" —— **故意保留**，因为真机数据在执行阶段才有。

### Type consistency

- `RankPair`：在 Task 2 定义，Task 3 / 4 / 5 / 7 全部一致使用 `Option<RankPair>` 形态 ✓
- `parse_rank`：Task 2 定义为 `fn parse_rank(s: &str) -> Option<RankPair>`；Task 3 内 `as_deref().and_then(parse_rank)` 一致 ✓
- `Gpa::fill_parsed`：Task 3 定义为 `pub fn fill_parsed(&mut self)`；Task 5 内 `g.fill_parsed()` 一致 ✓
- `SemesterKey` / `SemesterGpa` / `SemesterFailure`：Task 4 定义，Task 5 cmd_gpa_by_semester 一致使用 ✓
- `From<&Gpa> for SemesterGpa`：Task 4 定义返回 xnm/xqm 为空字符串，Task 5 调用方填充 —— 一致 ✓
- `GpaScope` / `GpaRank`：现有 apps/jwc 已定义，全 Plan 一致 import path `crate::apps::jwc::{GpaScope, GpaRank}` ✓
- `SEMESTER_QUERY_THROTTLE_MS = 600` + `XQM_LIST = &["3", "12", "16"]`：Task 5 内常量，Task 8 真机 smoke 验证耗时与此一致 ✓
