# T5 jwc 校历 iCal 导出 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 新增 `sjtu jwc calendar` 子命令，把当前学期个人课表 + 考试 + 学年校历统一导出为 RFC 5545 .ics（兼容 Google Calendar / Apple Calendar / Outlook / 移动本地导入 4 端）。

**Architecture:** 手卷 ~80-120 行 RFC 5545 writer，零新依赖。三路 fetch (N2151 课表 / N358105 考试 / 学年校历 T0 调研 endpoint 或 fixture) → 统一 `IcsEvent` → recurrence classifier 4 类决策 (RRULE FREQ=WEEKLY [;INTERVAL=2] 或 explode VEVENT) → writer 出 bytes (CRLF + 75-octet 折行 + 内嵌 VTIMEZONE Asia/Shanghai)。fail-soft：任一路失败 envelope `warnings[]`，exit 0。

**Tech Stack:** Rust 2021 / clap 4 / chrono (TimeZone + DateTime<Tz>) / serde / sha256-style hash 手卷 FNV-1a 64-bit ~12 行（**零新依赖决策**：UID 是幂等去重不要密码学强度，复用现有 crate 链）。

**OQ2 决策**（在 spec §10 留 plan 阶段解）：**手卷 FNV-1a 64-bit hash**。10 行 const fn 直接写在 events.rs，不引 `sha1` / `sha2` 依赖。UID 用 16 hex 字符（64-bit），碰撞概率 2^-32 远超学期事件总量 ~300。

---

## Task 0: T0 主对话 — chrome-devtools 调研学年校历 endpoint + fixture 落盘

> **主对话亲跑**（subagent 没 SJTU session + chrome-devtools 不可委派 + 需要用户协作切 tab）。

**Files:**
- Investigate: `tasks/isjtu_investigation.md` §8（新增章节）
- Create: `tests/fixtures/jwc/academic_calendar_2025_12.json`
- Create (if endpoint found): document N 系列 ID + form 字段到 §8

- [ ] **Step 1: chrome-devtools 半自动 SOP 调研**

主对话引导用户：
1. 浏览器登 i.sjtu.edu.cn 主菜单
2. 找"校历"相关入口（候选位置：教学服务 / 学生 / 帮助 / 主页底部链接）
3. chrome-devtools MCP `list_network_requests` 在用户切到校历页时抓 XHR
4. 关键判定：是否有 POST 到 `i.sjtu.edu.cn/<N\d+>.html` 的 N 系列 endpoint，且 response 含学期起止 / 节假日 / 调休信息

- [ ] **Step 2: 若挖到 N 系列 endpoint**

记录到 `tasks/isjtu_investigation.md` §8：
```markdown
## §8 学年校历 N 系列 endpoint（T5 T0 调研）

**URL**: POST https://i.sjtu.edu.cn/<NXXX>.html
**Auth**: jwc CAS (复用 sub_session)
**Request form**:
  - <字段 1>=<值 1>
  - <字段 2>=<值 2>
**Response 关键字段**：
  - xqkssj (学期开始日期，"YYYY-MM-DD")
  - xqjssj (学期结束日期)
  - jjr (节假日数组：[{rq, mc}])
  - tx (调休数组：[{src_rq, dst_rq}])
**真机 envelope 范本**：（脱敏后落 tests/fixtures/jwc/n<XXX>_academic_2025_12.json）
```

- [ ] **Step 3: 若没找到 N 系列（fallback fixture 路径）**

手工创建 `tests/fixtures/jwc/academic_calendar_2025_12.json`，以 SJTU 教务处官网或学校发布的春季学期校历为权威源：
```json
{
  "xnm": "2025",
  "xqm": "12",
  "xqkssj": "2026-02-23",
  "xqjssj": "2026-07-05",
  "jjr": [
    {"rq": "2026-04-04", "mc": "清明节"},
    {"rq": "2026-04-05", "mc": "清明节"},
    {"rq": "2026-04-06", "mc": "清明节调休"},
    {"rq": "2026-05-01", "mc": "劳动节"},
    {"rq": "2026-05-02", "mc": "劳动节"},
    {"rq": "2026-05-03", "mc": "劳动节"}
  ],
  "tx": []
}
```

记录到 `tasks/isjtu_investigation.md` §8：
```markdown
## §8 学年校历调研（T5 T0）

**结论**：调研 i.sjtu 整菜单未找到 N 系列学年校历 endpoint。
**回退方案**：每学期人工灌 `tests/fixtures/jwc/academic_calendar_<xnm>_<xqm>.json`。
**维护流程**：开学前从 jwc.sjtu.edu.cn 官方校历页拷数据 → 转 JSON → PR。
```

- [ ] **Step 4: 提交 fixture + investigation 章节**

```bash
git add tests/fixtures/jwc/academic_calendar_2025_12.json tasks/isjtu_investigation.md
git commit -m "chore(s5): T5 T0 学年校历调研 + 2025-12 fixture 落盘"
```

---

## Task 1: AcademicEvent struct + serde 单测

**Files:**
- Create: `src/apps/jwc/models/calendar.rs`
- Modify: `src/apps/jwc/models/mod.rs` (加 `pub mod calendar;`)

- [ ] **Step 1: 写 failing test**

新建 `src/apps/jwc/models/calendar.rs`：
```rust
//! §8 学年校历响应实体。endpoint 与 fixture 共用。

use serde::{Deserialize, Serialize};

/// 学年校历 envelope。字段名贴 fixture / 候选 N 系列 endpoint 响应（spec §1.3 + T0 调研）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AcademicCalendar {
    #[serde(default)]
    pub xnm: Option<String>,
    #[serde(default)]
    pub xqm: Option<String>,
    /// 学期开始日期 "YYYY-MM-DD"。
    #[serde(default)]
    pub xqkssj: Option<String>,
    /// 学期结束日期。
    #[serde(default)]
    pub xqjssj: Option<String>,
    /// 节假日清单。
    #[serde(default)]
    pub jjr: Vec<Holiday>,
    /// 调休清单。
    #[serde(default)]
    pub tx: Vec<MakeupClass>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Holiday {
    /// 日期 "YYYY-MM-DD"。
    #[serde(default)]
    pub rq: Option<String>,
    /// 节假日名称。
    #[serde(default)]
    pub mc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct MakeupClass {
    /// 源日期（不上课）。
    #[serde(default)]
    pub src_rq: Option<String>,
    /// 目标日期（上课替代）。
    #[serde(default)]
    pub dst_rq: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture_2025_12() {
        let raw = std::fs::read_to_string("tests/fixtures/jwc/academic_calendar_2025_12.json")
            .expect("fixture 缺失");
        let parsed: AcademicCalendar = serde_json::from_str(&raw).expect("parse");
        assert_eq!(parsed.xnm.as_deref(), Some("2025"));
        assert_eq!(parsed.xqm.as_deref(), Some("12"));
        assert!(parsed.jjr.len() >= 6, "至少 6 个节假日");
        let qm = parsed.jjr.iter().find(|h| h.mc.as_deref() == Some("清明节"));
        assert!(qm.is_some(), "应包含清明节");
    }

    #[test]
    fn empty_envelope_defaults_to_empty_vecs() {
        let parsed: AcademicCalendar = serde_json::from_str("{}").unwrap();
        assert!(parsed.jjr.is_empty());
        assert!(parsed.tx.is_empty());
    }
}
```

- [ ] **Step 2: 跑测试确认 fail（编译失败因 mod 没接）**

```bash
cargo test --lib jwc::models::calendar 2>&1 | tail -10
```
Expected: 编译错 `unresolved module declaration`（calendar 没在 mod.rs 暴露）

- [ ] **Step 3: 接 mod**

Modify `src/apps/jwc/models/mod.rs`：在末尾加：
```rust
pub mod calendar;
```

- [ ] **Step 4: 跑测试确认 pass**

```bash
cargo test --lib jwc::models::calendar -- --nocapture 2>&1 | tail -10
```
Expected: `test result: ok. 2 passed`

- [ ] **Step 5: fmt + clippy + 行数**

```bash
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && wc -l src/apps/jwc/models/calendar.rs
```
Expected: fmt 0 diff，clippy 0 warning，行数 < 100

- [ ] **Step 6: Commit**

```bash
git add src/apps/jwc/models/calendar.rs src/apps/jwc/models/mod.rs
git commit -m "feat(jwc): add AcademicCalendar / Holiday / MakeupClass models for T5"
```

---

## Task 2: writer.rs + vtimezone.rs RFC 5545 输出 + 单测

**Files:**
- Create: `src/commands/jwc/ical/mod.rs` (空 mod + pub mod 声明，~10 行)
- Create: `src/commands/jwc/ical/writer.rs` (~80 行)
- Create: `src/commands/jwc/ical/vtimezone.rs` (~30 行)
- Create: `src/commands/jwc/ical/tests.rs` (~100 行 起步)
- Modify: `src/commands/jwc/mod.rs` (加 `pub mod ical;`)

- [ ] **Step 1: 创建 vtimezone.rs 静态块**

新建 `src/commands/jwc/ical/vtimezone.rs`：
```rust
//! Asia/Shanghai VTIMEZONE 静态块（中国全年 UTC+8 无 DST）。
//!
//! 内嵌 VTIMEZONE 是 #1 时区兼容关键 —— TZID-only 不嵌在 Google/Apple/Outlook 上
//! 都会导致时区解析失败或回退到客户端本地时区（subagent 研究 2026-05-13 §4）。

/// Asia/Shanghai VTIMEZONE 块（不含换行）。writer 端拼时按 CRLF 加。
///
/// 字面照搬 IANA tzdb 的中国时区简化形态：
/// - DTSTART:19890101T000000（1989 年后中国停 DST）
/// - TZOFFSET +0800 恒定
pub fn vtimezone_block() -> &'static str {
    concat!(
        "BEGIN:VTIMEZONE\r\n",
        "TZID:Asia/Shanghai\r\n",
        "BEGIN:STANDARD\r\n",
        "DTSTART:19890101T000000\r\n",
        "TZOFFSETFROM:+0800\r\n",
        "TZOFFSETTO:+0800\r\n",
        "TZNAME:CST\r\n",
        "END:STANDARD\r\n",
        "END:VTIMEZONE\r\n",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_starts_and_ends_correctly() {
        let b = vtimezone_block();
        assert!(b.starts_with("BEGIN:VTIMEZONE\r\n"));
        assert!(b.ends_with("END:VTIMEZONE\r\n"));
        assert!(b.contains("TZID:Asia/Shanghai\r\n"));
        assert!(b.contains("TZOFFSETTO:+0800\r\n"));
    }

    #[test]
    fn block_uses_crlf_not_lf() {
        let b = vtimezone_block();
        assert!(!b.contains("\n\n"), "不应有连续换行");
        // 数 CRLF：应有 9 个（每行 1 个）
        assert_eq!(b.matches("\r\n").count(), 9);
    }
}
```

- [ ] **Step 2: 创建 writer.rs（RFC 5545 emit + CRLF + 75-octet fold）**

新建 `src/commands/jwc/ical/writer.rs`：
```rust
//! RFC 5545 .ics writer：CRLF 换行 + 75-octet 折行 + VCALENDAR/VEVENT 拼装。
//!
//! 关键硬规则（subagent 研究 2026-05-13）：
//! - CRLF 换行（LF 单独会被 Apple Calendar 静默丢事件）
//! - 75-octet 折行，续行以 SP 开头
//! - 多字节 UTF-8 字符不可在 octet 边界切开（按 char 边界整体保留）
//! - 内嵌 VTIMEZONE Asia/Shanghai
//! - PRODID 必填
//! - X-WR-CALNAME / X-WR-TIMEZONE Google 读其他忽略

use super::vtimezone::vtimezone_block;

/// 按 75 octet 折行；续行以 SP 起；多字节字符整体保留不切。
pub fn fold_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + line.len() / 75);
    let mut current_bytes = 0;
    let max = 75;
    for ch in line.chars() {
        let ch_bytes = ch.len_utf8();
        if current_bytes + ch_bytes > max {
            out.push_str("\r\n ");
            current_bytes = 1; // 续行的 SP 占 1 byte
        }
        out.push(ch);
        current_bytes += ch_bytes;
    }
    out
}

/// 单行 property 输出：fold + CRLF 结尾。
pub fn emit_line(buf: &mut String, line: &str) {
    buf.push_str(&fold_line(line));
    buf.push_str("\r\n");
}

/// VCALENDAR header（包含 VTIMEZONE）。
pub fn emit_header(buf: &mut String, calname: &str) {
    emit_line(buf, "BEGIN:VCALENDAR");
    emit_line(buf, "VERSION:2.0");
    emit_line(buf, "PRODID:-//sjtu-cli//SJTU iCal Export//EN");
    emit_line(buf, "CALSCALE:GREGORIAN");
    emit_line(buf, "METHOD:PUBLISH");
    emit_line(buf, &format!("X-WR-CALNAME:{}", calname));
    emit_line(buf, "X-WR-TIMEZONE:Asia/Shanghai");
    buf.push_str(vtimezone_block());
}

/// VCALENDAR footer。
pub fn emit_footer(buf: &mut String) {
    emit_line(buf, "END:VCALENDAR");
}

/// 把一个 VEVENT 加入 buf。各字段已由 events.rs 准备好为 RFC 5545 string。
///
/// `dtstart_local` / `dtend_local` 格式："20251015T080000"（local time 配 TZID 用）。
pub struct VEventFields<'a> {
    pub uid: &'a str,
    pub dtstamp_utc: &'a str, // "20260513T024105Z"
    pub dtstart_local: &'a str,
    pub dtend_local: &'a str,
    pub summary: &'a str,
    pub description: Option<&'a str>,
    pub location: Option<&'a str>,
    pub rrule: Option<&'a str>,
}

pub fn emit_vevent(buf: &mut String, e: &VEventFields) {
    emit_line(buf, "BEGIN:VEVENT");
    emit_line(buf, &format!("UID:{}", e.uid));
    emit_line(buf, &format!("DTSTAMP:{}", e.dtstamp_utc));
    emit_line(buf, &format!("DTSTART;TZID=Asia/Shanghai:{}", e.dtstart_local));
    emit_line(buf, &format!("DTEND;TZID=Asia/Shanghai:{}", e.dtend_local));
    emit_line(buf, &format!("SUMMARY:{}", escape_text(e.summary)));
    if let Some(d) = e.description {
        emit_line(buf, &format!("DESCRIPTION:{}", escape_text(d)));
    }
    if let Some(l) = e.location {
        emit_line(buf, &format!("LOCATION:{}", escape_text(l)));
    }
    if let Some(r) = e.rrule {
        emit_line(buf, &format!("RRULE:{}", r));
    }
    emit_line(buf, "END:VEVENT");
}

/// RFC 5545 §3.3.11：TEXT 类型必须转义 `\` / `;` / `,` / 换行。
fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {} // 吞掉
            _ => out.push(ch),
        }
    }
    out
}
```

- [ ] **Step 3: 创建 mod.rs + tests.rs 框架**

新建 `src/commands/jwc/ical/mod.rs`：
```rust
//! `sjtu jwc calendar` 子命令的实现入口：把课表 + 考试 + 学年校历导出为 RFC 5545 .ics。

pub mod events;
pub mod recurrence;
pub mod vtimezone;
pub mod writer;

#[cfg(test)]
mod tests;
```

新建 `src/commands/jwc/ical/tests.rs`：
```rust
//! ical writer / recurrence / events 单测集合。

use super::vtimezone::vtimezone_block;
use super::writer::{emit_line, emit_vevent, fold_line, VEventFields};

#[test]
fn fold_line_does_not_fold_short_lines() {
    let short = "SUMMARY:短课名";
    let folded = fold_line(short);
    assert!(!folded.contains("\r\n"), "短行不应折");
    assert_eq!(folded, short);
}

#[test]
fn fold_line_folds_at_75_octets_for_ascii() {
    let line = "X-CUSTOM:".to_string() + &"a".repeat(200);
    let folded = fold_line(&line);
    let parts: Vec<&str> = folded.split("\r\n").collect();
    // 首行 75 octet，续行每个 74（1 byte SP + 74 content）
    assert!(parts.len() >= 3);
    assert_eq!(parts[0].len(), 75);
    for p in &parts[1..] {
        assert!(p.starts_with(' '), "续行必须以 SP 起");
        assert!(p.len() <= 75);
    }
}

#[test]
fn fold_line_does_not_split_multibyte_chars() {
    // 课程"操作系统原理"+ padding 让边界落在中文字内
    let pad = "A".repeat(70);
    let line = format!("SUMMARY:{}操作系统原理", pad);
    let folded = fold_line(&line);
    // 解 fold 重组，断言中文字串保留完整
    let unfolded = folded.replace("\r\n ", "");
    assert_eq!(unfolded, line);
    // 检查没有断在 UTF-8 中字节
    for part in folded.split("\r\n") {
        if !part.is_empty() {
            assert!(std::str::from_utf8(part.as_bytes()).is_ok());
        }
    }
}

#[test]
fn emit_line_appends_crlf() {
    let mut buf = String::new();
    emit_line(&mut buf, "TEST:x");
    assert_eq!(buf, "TEST:x\r\n");
}

#[test]
fn vtimezone_block_is_well_formed() {
    let b = vtimezone_block();
    assert!(b.starts_with("BEGIN:VTIMEZONE\r\n"));
    assert!(b.contains("TZOFFSETTO:+0800\r\n"));
}

#[test]
fn emit_vevent_includes_required_fields() {
    let mut buf = String::new();
    emit_vevent(
        &mut buf,
        &VEventFields {
            uid: "abc123@sjtu-cli",
            dtstamp_utc: "20260513T024105Z",
            dtstart_local: "20251015T080000",
            dtend_local: "20251015T084500",
            summary: "操作系统",
            description: Some("理论课"),
            location: Some("东上院 102"),
            rrule: Some("FREQ=WEEKLY;COUNT=18"),
        },
    );
    assert!(buf.contains("UID:abc123@sjtu-cli\r\n"));
    assert!(buf.contains("DTSTART;TZID=Asia/Shanghai:20251015T080000\r\n"));
    assert!(buf.contains("SUMMARY:操作系统\r\n"));
    assert!(buf.contains("RRULE:FREQ=WEEKLY;COUNT=18\r\n"));
}

#[test]
fn emit_vevent_escapes_special_chars() {
    let mut buf = String::new();
    emit_vevent(
        &mut buf,
        &VEventFields {
            uid: "x@sjtu-cli",
            dtstamp_utc: "20260513T024105Z",
            dtstart_local: "20251015T080000",
            dtend_local: "20251015T084500",
            summary: "课;有,逗号\\反斜杠",
            description: None,
            location: None,
            rrule: None,
        },
    );
    assert!(buf.contains(r"SUMMARY:课\;有\,逗号\\反斜杠"));
}
```

- [ ] **Step 4: 接 commands/jwc/mod.rs**

Modify `src/commands/jwc/mod.rs`：加 `pub mod ical;`

- [ ] **Step 5: 跑测试**

```bash
cargo test --lib jwc::ical 2>&1 | tail -15
```
Expected: `test result: ok. 7 passed`（vtimezone 2 + tests.rs 5 = 7 单测 + writer.rs 内 0 + 接 vtimezone mod test 2，合 9）

- [ ] **Step 6: fmt + clippy + 行数**

```bash
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && wc -l src/commands/jwc/ical/*.rs
```
Expected: 0 diff / 0 warn / writer.rs ~95 / vtimezone.rs ~50 / tests.rs ~100 / mod.rs ~12，全 < 200

- [ ] **Step 7: Commit**

```bash
git add src/commands/jwc/ical/ src/commands/jwc/mod.rs
git commit -m "feat(jwc): add ical writer + vtimezone + 9 unit tests (T5 step 2)"
```

---

## Task 3: recurrence.rs 4 类周次决策 + 单测

**Files:**
- Create: `src/commands/jwc/ical/recurrence.rs` (~70 行)
- Modify: `src/commands/jwc/ical/mod.rs` (`pub mod recurrence;` 已加，无需再改)
- Modify: `src/commands/jwc/ical/tests.rs` (+~80 行 recurrence 单测)

- [ ] **Step 1: 创建 recurrence.rs**

新建 `src/commands/jwc/ical/recurrence.rs`：
```rust
//! N2151 KbItem.zcd 周次字符串 → 4 类决策（spec §7）。
//!
//! - A. 全学期连续 "1-18周" → RRULE FREQ=WEEKLY;COUNT=18
//! - B. 规则单/双周 "1-18周(单)" / "(双)" → INTERVAL=2
//! - C. 离散周次 "3,5,7,11周" → explode VEVENT
//! - D. 短开范围 "1-3周" (≤3 周) → explode

/// recurrence 决策。
#[derive(Debug, Clone, PartialEq)]
pub enum Recurrence {
    /// FREQ=WEEKLY;COUNT=N。
    Weekly { count: u32 },
    /// FREQ=WEEKLY;INTERVAL=2;COUNT=N（起始周由 events.rs 算 first_week）。
    Biweekly { count: u32, first_week: u32 },
    /// 不规则离散周次（events.rs 端 explode）。
    Discrete { weeks: Vec<u32> },
}

/// 解析 zcd → Recurrence。zcd 例值：
/// - "1-18周"           → Weekly { count: 18 }
/// - "1-18周(单)"       → Biweekly { count: 9, first_week: 1 }
/// - "2-18周(双)"       → Biweekly { count: 9, first_week: 2 }
/// - "3,5,7,11周"       → Discrete { weeks: [3,5,7,11] }
/// - "1-3周"            → Discrete { weeks: [1,2,3] }（≤3 短开走 explode）
/// - 解析失败 → 空 Discrete（events.rs 上游 fail-soft）
pub fn parse_zcd(zcd: &str) -> Recurrence {
    let trimmed = zcd.trim().trim_end_matches('周');
    // 优先识别括号修饰
    let (range_or_list, parity) = if let Some((head, _)) = trimmed.split_once('(') {
        let parity = if trimmed.ends_with("单)") {
            Some(1u32) // odd
        } else if trimmed.ends_with("双)") {
            Some(2u32) // even
        } else {
            None
        };
        (head.trim_end_matches('周'), parity)
    } else {
        (trimmed, None)
    };

    // 范围 "a-b" or 离散 "a,b,c"
    if range_or_list.contains(',') {
        let weeks: Vec<u32> = range_or_list
            .split(',')
            .filter_map(|s| s.trim().parse::<u32>().ok())
            .collect();
        Recurrence::Discrete { weeks }
    } else if let Some((a, b)) = range_or_list.split_once('-') {
        let start: u32 = a.trim().parse().unwrap_or(0);
        let end: u32 = b.trim().parse().unwrap_or(0);
        if start == 0 || end < start {
            return Recurrence::Discrete { weeks: vec![] };
        }
        let span = end - start + 1;
        match parity {
            None if span <= 3 => Recurrence::Discrete {
                weeks: (start..=end).collect(),
            },
            None => Recurrence::Weekly { count: span },
            Some(_) => {
                let count = span.div_ceil(2);
                Recurrence::Biweekly {
                    count,
                    first_week: start,
                }
            }
        }
    } else {
        // 单 "5周" 之类，按离散单元素处理
        match range_or_list.trim().parse::<u32>() {
            Ok(w) => Recurrence::Discrete { weeks: vec![w] },
            Err(_) => Recurrence::Discrete { weeks: vec![] },
        }
    }
}

/// 把 Recurrence::Weekly / Biweekly 翻译成 RRULE 字符串。Discrete 返 None（explode 路径不用 RRULE）。
pub fn to_rrule(rec: &Recurrence) -> Option<String> {
    match rec {
        Recurrence::Weekly { count } => Some(format!("FREQ=WEEKLY;COUNT={}", count)),
        Recurrence::Biweekly { count, .. } => Some(format!("FREQ=WEEKLY;INTERVAL=2;COUNT={}", count)),
        Recurrence::Discrete { .. } => None,
    }
}
```

- [ ] **Step 2: 加单测到 tests.rs**

Append to `src/commands/jwc/ical/tests.rs`：
```rust
use super::recurrence::{parse_zcd, to_rrule, Recurrence};

#[test]
fn parse_zcd_full_semester() {
    assert_eq!(parse_zcd("1-18周"), Recurrence::Weekly { count: 18 });
}

#[test]
fn parse_zcd_odd_weeks() {
    assert_eq!(
        parse_zcd("1-18周(单)"),
        Recurrence::Biweekly {
            count: 9,
            first_week: 1
        }
    );
}

#[test]
fn parse_zcd_even_weeks() {
    assert_eq!(
        parse_zcd("2-18周(双)"),
        Recurrence::Biweekly {
            count: 9,
            first_week: 2
        }
    );
}

#[test]
fn parse_zcd_discrete_list() {
    assert_eq!(
        parse_zcd("3,5,7,11周"),
        Recurrence::Discrete {
            weeks: vec![3, 5, 7, 11]
        }
    );
}

#[test]
fn parse_zcd_short_range_explodes() {
    // 1-3 周 ≤ 3，走 explode 而非 RRULE COUNT=3
    assert_eq!(
        parse_zcd("1-3周"),
        Recurrence::Discrete {
            weeks: vec![1, 2, 3]
        }
    );
}

#[test]
fn parse_zcd_garbage_returns_empty_discrete() {
    assert_eq!(parse_zcd("无效"), Recurrence::Discrete { weeks: vec![] });
}

#[test]
fn to_rrule_weekly() {
    let r = to_rrule(&Recurrence::Weekly { count: 18 }).unwrap();
    assert_eq!(r, "FREQ=WEEKLY;COUNT=18");
}

#[test]
fn to_rrule_biweekly() {
    let r = to_rrule(&Recurrence::Biweekly {
        count: 9,
        first_week: 1,
    })
    .unwrap();
    assert_eq!(r, "FREQ=WEEKLY;INTERVAL=2;COUNT=9");
}

#[test]
fn to_rrule_discrete_returns_none() {
    assert!(to_rrule(&Recurrence::Discrete { weeks: vec![1, 3] }).is_none());
}
```

- [ ] **Step 3: 跑测试**

```bash
cargo test --lib jwc::ical::tests::parse_zcd 2>&1 | tail -10
cargo test --lib jwc::ical 2>&1 | tail -15
```
Expected: 9 个 recurrence 测全过 + 之前 writer 测全过

- [ ] **Step 4: fmt + clippy + 行数**

```bash
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && wc -l src/commands/jwc/ical/recurrence.rs src/commands/jwc/ical/tests.rs
```
Expected: 0 diff / 0 warn / recurrence.rs ~80 / tests.rs ~190 < 300

- [ ] **Step 5: Commit**

```bash
git add src/commands/jwc/ical/recurrence.rs src/commands/jwc/ical/tests.rs
git commit -m "feat(jwc): add recurrence classifier (4 类) + 9 unit tests (T5 step 3)"
```

---

## Task 4: events.rs IcsEvent unify + FNV-1a UID + 单测

**Files:**
- Create: `src/commands/jwc/ical/events.rs` (~120 行)
- Modify: `src/commands/jwc/ical/tests.rs` (+~50 行 events 单测)

- [ ] **Step 1: 创建 events.rs**

新建 `src/commands/jwc/ical/events.rs`：
```rust
//! 三路数据（KbItem 课表 / Exam 考试 / AcademicCalendar 校历）→ 统一 IcsEvent → VEvent stream。
//!
//! UID 算法：FNV-1a 64-bit 手卷 hash（spec OQ2 决策：零新依赖，UID 用 hex 16 字符，
//! 学期事件 ~300，碰撞概率 2^-32 远超需求）。
//!
//! recurrence 字段：
//! - Class：parse_zcd(kb.zcd) → 决定 RRULE 或 explode
//! - Exam / AcademicEvent：永远 None

use crate::apps::jwc::models::calendar::AcademicCalendar;
use crate::apps::jwc::models::exam::Exam;
use crate::apps::jwc::models::schedule::KbItem;
use crate::apps::jwc::period_clock::lookup;
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};

use super::recurrence::{parse_zcd, to_rrule, Recurrence};

#[derive(Debug, Clone, PartialEq)]
pub enum IcsKind {
    Class,
    Exam,
    Academic,
}

#[derive(Debug, Clone)]
pub struct IcsEvent {
    pub uid_seed: String,
    pub summary: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub kind: IcsKind,
    pub dtstart: NaiveDateTime,
    pub dtend: NaiveDateTime,
    pub recurrence: Option<Recurrence>,
}

/// FNV-1a 64-bit hash → 16 char hex。UID 用，零依赖。
pub fn fnv1a_64(s: &str) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    }
    format!("{:016x}", h)
}

pub fn make_uid(seed: &str) -> String {
    format!("{}@sjtu-cli", fnv1a_64(seed))
}

/// KbItem → IcsEvent。需要 term_start（学期第 1 周周一日期，从 AcademicCalendar.xqkssj 推）。
/// 失败（jc 越界 / 缺字段 / parse 错）返 None。
pub fn from_kb_item(kb: &KbItem, xnm: &str, xqm: &str, term_start: NaiveDate) -> Option<IcsEvent> {
    let xqj: u8 = kb.xqj.as_deref()?.parse().ok()?;
    if !(1..=7).contains(&xqj) {
        return None;
    }
    let jc: &str = kb.jc.as_deref()?;
    // 节次形如 "1-2" / "3-4" / "11-12"
    let (start_jc, end_jc) = jc.split_once('-')
        .and_then(|(a, b)| Some((a.parse::<u8>().ok()?, b.parse::<u8>().ok()?)))?;
    let (start_time, _) = lookup(start_jc)?;
    let (_, end_time) = lookup(end_jc)?;

    let zcd = kb.zcd.as_deref().unwrap_or("");
    let recurrence = Some(parse_zcd(zcd));

    // 第 1 周该周几日期 = term_start (周一) + (xqj - 1)
    let first_day = term_start + chrono::Duration::days(xqj as i64 - 1);
    let dtstart = first_day.and_time(start_time);
    let dtend = first_day.and_time(end_time);

    let summary = kb.kcmc.as_deref().unwrap_or("课").to_string();
    let location = kb.cdmc.clone().filter(|s| !s.is_empty());
    let teacher = kb.jsxm.as_deref();
    let description = teacher.map(|t| format!("教师: {}", t));

    let uid_seed = format!(
        "{}_{}_class_{}_xqj{}_jc{}_zcd{}",
        xnm,
        xqm,
        kb.kch.as_deref().unwrap_or("?"),
        xqj,
        jc,
        zcd
    );

    Some(IcsEvent {
        uid_seed,
        summary,
        description,
        location,
        kind: IcsKind::Class,
        dtstart,
        dtend,
        recurrence,
    })
}

/// Exam.kssj 格式："YYYY-MM-DD(HH:MM-HH:MM)" → IcsEvent。
pub fn from_exam(exam: &Exam, xnm: &str, xqm: &str) -> Option<IcsEvent> {
    let kssj = exam.kssj.as_deref()?;
    let (date_str, time_part) = kssj.split_once('(')?;
    let date = NaiveDate::parse_from_str(date_str.trim(), "%Y-%m-%d").ok()?;
    let time_part = time_part.trim_end_matches(')');
    let (start_str, end_str) = time_part.split_once('-')?;
    let start = NaiveTime::parse_from_str(start_str.trim(), "%H:%M").ok()?;
    let end = NaiveTime::parse_from_str(end_str.trim(), "%H:%M").ok()?;

    let summary = format!(
        "[考] {}",
        exam.kcmc.as_deref().unwrap_or("未知考试")
    );
    let location = exam.cdmc.clone().filter(|s| !s.is_empty());
    let description = exam
        .ksmc
        .clone()
        .or_else(|| Some(format!("学期 {}/{}", xnm, xqm)));

    let uid_seed = format!(
        "{}_{}_exam_{}_{}",
        xnm,
        xqm,
        exam.kch.as_deref().unwrap_or("?"),
        date.format("%Y%m%d")
    );

    Some(IcsEvent {
        uid_seed,
        summary,
        description,
        location,
        kind: IcsKind::Exam,
        dtstart: date.and_time(start),
        dtend: date.and_time(end),
        recurrence: None,
    })
}

/// AcademicCalendar.jjr → Vec<IcsEvent>（每节假日 1 整天 VEVENT）。
pub fn from_academic(cal: &AcademicCalendar, xnm: &str, xqm: &str) -> Vec<IcsEvent> {
    cal.jjr
        .iter()
        .filter_map(|h| {
            let rq = h.rq.as_deref()?;
            let date = NaiveDate::parse_from_str(rq, "%Y-%m-%d").ok()?;
            let mc = h.mc.as_deref().unwrap_or("假期");
            // 整天事件：DTSTART/DTEND 跨当天 0:00 - 23:59
            Some(IcsEvent {
                uid_seed: format!("{}_{}_holiday_{}", xnm, xqm, date.format("%Y%m%d")),
                summary: format!("[校历] {}", mc),
                description: None,
                location: None,
                kind: IcsKind::Academic,
                dtstart: date.and_hms_opt(0, 0, 0)?,
                dtend: date.and_hms_opt(23, 59, 0)?,
                recurrence: None,
            })
        })
        .collect()
}

/// 推学期第 1 周周一日期。xqkssj 是学期开始日期（可能不是周一），回退到该日期所在 ISO 周的周一。
pub fn term_first_monday(xqkssj: &str) -> Option<NaiveDate> {
    let d = NaiveDate::parse_from_str(xqkssj, "%Y-%m-%d").ok()?;
    let weekday_num = d.weekday().num_days_from_monday() as i64;
    Some(d - chrono::Duration::days(weekday_num))
}
```

需要 `KbItem.cdmc` (教室)：spec §1.1 提到，subagent 实装时如发现 KbItem 上无该字段，按 `models/schedule.rs` 既有 KbItem 字段为准（可能是 `cdmc` 或 `jxdd`）；如缺，事件 location=None，不阻塞。

- [ ] **Step 2: 加 events 单测**

Append to `src/commands/jwc/ical/tests.rs`：
```rust
use super::events::{fnv1a_64, from_academic, from_exam, from_kb_item, make_uid, term_first_monday};
use crate::apps::jwc::models::calendar::{AcademicCalendar, Holiday};
use crate::apps::jwc::models::exam::Exam;
use crate::apps::jwc::models::schedule::KbItem;
use chrono::NaiveDate;

#[test]
fn fnv1a_64_known_vector() {
    // "foobar" 的 FNV-1a 64-bit 是 0x85944171f73967e8（标准 testvec）
    assert_eq!(fnv1a_64("foobar"), "85944171f73967e8");
}

#[test]
fn make_uid_appends_domain() {
    let uid = make_uid("test");
    assert!(uid.ends_with("@sjtu-cli"));
    assert_eq!(uid.len(), 16 + 1 + "sjtu-cli".len());
}

#[test]
fn make_uid_is_deterministic() {
    let a = make_uid("xnm_xqm_class_KCH_xqj1_jc1-2_zcd1-18周");
    let b = make_uid("xnm_xqm_class_KCH_xqj1_jc1-2_zcd1-18周");
    assert_eq!(a, b);
}

#[test]
fn make_uid_differs_for_different_seeds() {
    assert_ne!(make_uid("a"), make_uid("b"));
}

#[test]
fn term_first_monday_handles_non_monday_start() {
    // 2026-02-23 是周一 → 返回自身
    assert_eq!(
        term_first_monday("2026-02-23"),
        NaiveDate::from_ymd_opt(2026, 2, 23)
    );
    // 2026-02-25 周三 → 回退到 2026-02-23
    assert_eq!(
        term_first_monday("2026-02-25"),
        NaiveDate::from_ymd_opt(2026, 2, 23)
    );
}

#[test]
fn from_academic_yields_one_event_per_holiday() {
    let cal = AcademicCalendar {
        jjr: vec![
            Holiday {
                rq: Some("2026-04-04".into()),
                mc: Some("清明节".into()),
            },
            Holiday {
                rq: Some("2026-05-01".into()),
                mc: Some("劳动节".into()),
            },
        ],
        ..Default::default()
    };
    let events = from_academic(&cal, "2025", "12");
    assert_eq!(events.len(), 2);
    assert!(events[0].summary.contains("清明节"));
}

#[test]
fn from_exam_parses_compound_kssj() {
    let exam = Exam {
        kssj: Some("2026-06-15(09:00-11:00)".into()),
        kcmc: Some("操作系统".into()),
        kch: Some("CS0001".into()),
        cdmc: Some("东上院 102".into()),
        ksmc: Some("2025-2026-2 期末考试".into()),
        ..Default::default()
    };
    let e = from_exam(&exam, "2025", "12").expect("parse");
    assert_eq!(e.summary, "[考] 操作系统");
    assert_eq!(e.location.as_deref(), Some("东上院 102"));
    assert_eq!(
        e.dtstart,
        NaiveDate::from_ymd_opt(2026, 6, 15)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap()
    );
    assert!(e.recurrence.is_none());
}
```

注意：`Exam` 需要 `cdmc` (考场) 字段；subagent 检查 `models/exam.rs`，若实际 field 名不同（比如 `kscd` / `kchdcd`），按真名调整。subagent 阅 `src/apps/jwc/models/exam.rs` 后取真实字段。

- [ ] **Step 3: 跑测试**

```bash
cargo test --lib jwc::ical 2>&1 | tail -20
```
Expected: writer 7 + recurrence 9 + events 7 = 23 测全过

- [ ] **Step 4: fmt + clippy + 行数**

```bash
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && wc -l src/commands/jwc/ical/*.rs
```
Expected: events.rs ~150 / tests.rs ~280 < 300 / 其他不变

- [ ] **Step 5: Commit**

```bash
git add src/commands/jwc/ical/events.rs src/commands/jwc/ical/tests.rs
git commit -m "feat(jwc): IcsEvent unify + FNV-1a UID + 7 events tests (T5 step 4)"
```

---

## Task 5: api/calendar.rs endpoint loader + fixture loader 双轨 + 单测

**Files:**
- Create: `src/apps/jwc/api/calendar.rs` (~80 行)
- Modify: `src/apps/jwc/api/mod.rs` (加 `mod calendar;`)

- [ ] **Step 1: 创建 api/calendar.rs**

新建 `src/apps/jwc/api/calendar.rs`：
```rust
//! §8 学年校历获取入口。endpoint / fixture 双轨：
//!
//! - 若 T0 调研出 N 系列 endpoint → 走 `client.academic_calendar(xnm, xqm)` (HTTP)
//! - 否则 fallback 读 `tests/fixtures/jwc/academic_calendar_<xnm>_<xqm>.json`（开发期）
//!   或 `~/.sjtu-cli/academic_calendars/<xnm>_<xqm>.json`（部署期，用户自维护）
//!
//! T0 调研结论（2026-05-13）：<endpoint 是否找到，subagent 实装时按 T0 提交的
//! tasks/isjtu_investigation.md §8 章节决定走哪条>

use crate::apps::jwc::models::calendar::AcademicCalendar;
use anyhow::{Context, Result};
use std::path::PathBuf;

/// Fixture 优先的加载策略（T0 endpoint 调研失败时唯一路径）。
///
/// 查找顺序：
/// 1. `tests/fixtures/jwc/academic_calendar_<xnm>_<xqm>.json`（当前工作目录相对）
/// 2. `<config_dir>/academic_calendars/<xnm>_<xqm>.json`（部署期）
/// 3. 返 Err（events.rs 上游 fail-soft）
pub fn load_from_fixture(xnm: &str, xqm: &str) -> Result<AcademicCalendar> {
    let candidates: Vec<PathBuf> = vec![
        PathBuf::from(format!(
            "tests/fixtures/jwc/academic_calendar_{}_{}.json",
            xnm, xqm
        )),
        crate::config::config_dir()
            .ok()
            .map(|d| d.join("academic_calendars").join(format!("{}_{}.json", xnm, xqm)))
            .unwrap_or_default(),
    ];
    for path in &candidates {
        if path.exists() {
            let raw = std::fs::read_to_string(path)
                .with_context(|| format!("读 {} 失败", path.display()))?;
            let cal: AcademicCalendar =
                serde_json::from_str(&raw).with_context(|| format!("parse {} 失败", path.display()))?;
            return Ok(cal);
        }
    }
    Err(anyhow::anyhow!(
        "未找到 {} 学年 {} 学期的校历 fixture（检查路径: {:?}）",
        xnm,
        xqm,
        candidates
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_2025_12_fixture_from_repo() {
        // 仓库根目录运行 cargo test 时 CWD = repo root
        let cal = load_from_fixture("2025", "12").expect("fixture 已落 task 0");
        assert_eq!(cal.xnm.as_deref(), Some("2025"));
        assert!(!cal.jjr.is_empty());
    }

    #[test]
    fn missing_fixture_returns_err() {
        let res = load_from_fixture("9999", "99");
        assert!(res.is_err());
        let msg = format!("{:#}", res.unwrap_err());
        assert!(msg.contains("未找到"));
    }
}
```

> **subagent 注意**：T0 调研若挖到 N 系列 endpoint，**在 fixture loader 之外**再加一个 `pub async fn academic_calendar_from_api(client: &Client, xnm: &str, xqm: &str) -> Result<AcademicCalendar>`，参照 `src/apps/jwc/api/exams.rs` 的 `post_form_json` pattern。cmd_calendar 优先试 API，失败再走 fixture。如 T0 未挖到，本 Task 不实装 endpoint 入口。

- [ ] **Step 2: 接 api/mod.rs**

Modify `src/apps/jwc/api/mod.rs`：在 `mod` 列表加 `mod calendar;` 和 `pub use calendar::load_from_fixture;`（按现有 re-export 风格）

- [ ] **Step 3: 跑测试**

```bash
cargo test --lib jwc::api::calendar 2>&1 | tail -10
```
Expected: 2 个 fixture loader 测全过

- [ ] **Step 4: fmt + clippy + 行数**

```bash
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && wc -l src/apps/jwc/api/calendar.rs
```
Expected: ~80 行 < 200

- [ ] **Step 5: Commit**

```bash
git add src/apps/jwc/api/calendar.rs src/apps/jwc/api/mod.rs
git commit -m "feat(jwc): add academic_calendar fixture loader + 2 tests (T5 step 5)"
```

---

## Task 6: cmd_calendar handler + envelope 模式 + fail-soft

**Files:**
- Modify: `src/commands/jwc/ical/mod.rs` (扩到 ~120 行：加 `cmd_calendar` + `CalendarData`)
- Modify: `src/commands/jwc/mod.rs` (加 `pub use ical::cmd_calendar;`)

- [ ] **Step 1: 重写 ical/mod.rs 加入 handler**

替换 `src/commands/jwc/ical/mod.rs` 全文：
```rust
//! `sjtu jwc calendar` 子命令实装入口。
//!
//! 数据流：
//!   CLI args → 推默认 xnm/xqm → 并行 3 fetch → unify IcsEvent → emit .ics bytes → stdout / --to / --json envelope

pub mod events;
pub mod recurrence;
pub mod vtimezone;
pub mod writer;

#[cfg(test)]
mod tests;

use crate::apps::jwc::api::calendar::load_from_fixture;
use crate::apps::jwc::models::calendar::AcademicCalendar;
use crate::apps::jwc::Client;
use crate::output::{render, Envelope, OutputFormat};
use anyhow::Result;
use chrono::Utc;
use serde::Serialize;

use events::{from_academic, from_exam, from_kb_item, make_uid, term_first_monday, IcsEvent, IcsKind};
use recurrence::{to_rrule, Recurrence};
use writer::{emit_footer, emit_header, emit_vevent, VEventFields};

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CalendarData {
    pub xnm: String,
    pub xqm: String,
    pub event_count: usize,
    pub by_kind: ByKind,
    pub sha256_hex: String,
    pub bytes: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct ByKind {
    pub class: usize,
    pub exam: usize,
    pub academic: usize,
}

/// `sjtu jwc calendar` 入口。
pub async fn cmd_calendar(
    client: &Client,
    xnm: Option<String>,
    xqm: Option<String>,
    to: Option<std::path::PathBuf>,
    no_academic: bool,
    no_exams: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    // 1) 推默认学期
    let (xnm, xqm) = match (xnm, xqm) {
        (Some(a), Some(b)) => (a, b),
        _ => {
            use crate::apps::jwc::api::term::default_xnm_xqm_by_date;
            default_xnm_xqm_by_date(chrono::Local::now().date_naive())
        }
    };

    let mut warnings: Vec<String> = vec![];

    // 2) 并行 3 fetch（结构：(class, exam, academic)）
    let class_fut = client.schedule(&xnm, &xqm);
    let exam_fut = async {
        if no_exams {
            Ok(vec![])
        } else {
            client.exams(&xnm, &xqm).await
        }
    };
    let academic_fut = async {
        if no_academic {
            Ok(AcademicCalendar::default())
        } else {
            tokio::task::spawn_blocking({
                let xnm = xnm.clone();
                let xqm = xqm.clone();
                move || load_from_fixture(&xnm, &xqm)
            })
            .await
            .unwrap_or_else(|e| Err(anyhow::anyhow!("spawn_blocking: {}", e)))
        }
    };

    let (class_r, exam_r, academic_r) = tokio::join!(class_fut, exam_fut, academic_fut);

    let schedule = class_r.unwrap_or_else(|e| {
        warnings.push(format!("课表 (N2151) 失败: {:#}", e));
        Default::default()
    });
    let exams = exam_r.unwrap_or_else(|e| {
        warnings.push(format!("考试 (N358105) 失败: {:#}", e));
        vec![]
    });
    let academic = academic_r.unwrap_or_else(|e| {
        warnings.push(format!("学年校历 fallback fixture 失败: {:#}", e));
        AcademicCalendar::default()
    });

    // 3) unify → IcsEvent
    let term_start = academic
        .xqkssj
        .as_deref()
        .and_then(term_first_monday)
        .unwrap_or_else(|| chrono::Local::now().date_naive());
    let mut all_events: Vec<IcsEvent> = vec![];

    for kb in &schedule.kb_list {
        if let Some(ev) = from_kb_item(kb, &xnm, &xqm, term_start) {
            all_events.push(ev);
        }
    }
    for ex in &exams {
        if let Some(ev) = from_exam(ex, &xnm, &xqm) {
            all_events.push(ev);
        }
    }
    all_events.extend(from_academic(&academic, &xnm, &xqm));

    // 4) recurrence 展开 + writer emit
    let calname = format!("SJTU {}-{} 课表 + 考试 + 校历", xnm, xqm);
    let mut buf = String::new();
    emit_header(&mut buf, &calname);

    let dtstamp_utc = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let mut by_kind = ByKind::default();
    let mut total_count = 0;

    for ev in &all_events {
        match &ev.recurrence {
            Some(Recurrence::Discrete { weeks }) => {
                for &w in weeks {
                    emit_one_explode(&mut buf, ev, w, &dtstamp_utc);
                    total_count += 1;
                    bump_count(&mut by_kind, &ev.kind);
                }
            }
            other => {
                emit_one(&mut buf, ev, other.as_ref(), &dtstamp_utc);
                total_count += 1;
                bump_count(&mut by_kind, &ev.kind);
            }
        }
    }

    emit_footer(&mut buf);
    let bytes = buf.as_bytes();
    let sha256_hex = format!("{:016x}", events::fnv1a_64(buf.as_str()).parse::<u64>().unwrap_or(0)); // 用 FNV-1a 同步算 hash（不是密码学 SHA-256 但 envelope 标识够用）

    // 5) 输出
    if let Some(path) = to {
        std::fs::write(&path, bytes)?;
    }

    if fmt == Some(OutputFormat::Json) || fmt == Some(OutputFormat::Yaml) || to.is_some() {
        // envelope 模式
        let data = CalendarData {
            xnm,
            xqm,
            event_count: total_count,
            by_kind,
            sha256_hex,
            bytes: bytes.len(),
            warnings,
        };
        render(Envelope::ok(data), fmt)
    } else {
        // raw .ics → stdout
        use std::io::Write;
        std::io::stdout().write_all(bytes)?;
        Ok(())
    }
}

fn emit_one(buf: &mut String, ev: &IcsEvent, rec: Option<&Recurrence>, dtstamp: &str) {
    let uid = make_uid(&ev.uid_seed);
    let dtstart_local = ev.dtstart.format("%Y%m%dT%H%M%S").to_string();
    let dtend_local = ev.dtend.format("%Y%m%dT%H%M%S").to_string();
    let rrule = rec.and_then(to_rrule);
    emit_vevent(
        buf,
        &VEventFields {
            uid: &uid,
            dtstamp_utc: dtstamp,
            dtstart_local: &dtstart_local,
            dtend_local: &dtend_local,
            summary: &ev.summary,
            description: ev.description.as_deref(),
            location: ev.location.as_deref(),
            rrule: rrule.as_deref(),
        },
    );
}

fn emit_one_explode(buf: &mut String, ev: &IcsEvent, week: u32, dtstamp: &str) {
    let week_offset_days = (week as i64 - 1) * 7;
    let dtstart = ev.dtstart + chrono::Duration::days(week_offset_days);
    let dtend = ev.dtend + chrono::Duration::days(week_offset_days);
    let uid_seed_w = format!("{}_w{}", ev.uid_seed, week);
    let uid = make_uid(&uid_seed_w);
    let dtstart_local = dtstart.format("%Y%m%dT%H%M%S").to_string();
    let dtend_local = dtend.format("%Y%m%dT%H%M%S").to_string();
    emit_vevent(
        buf,
        &VEventFields {
            uid: &uid,
            dtstamp_utc: dtstamp,
            dtstart_local: &dtstart_local,
            dtend_local: &dtend_local,
            summary: &ev.summary,
            description: ev.description.as_deref(),
            location: ev.location.as_deref(),
            rrule: None,
        },
    );
}

fn bump_count(by: &mut ByKind, kind: &IcsKind) {
    match kind {
        IcsKind::Class => by.class += 1,
        IcsKind::Exam => by.exam += 1,
        IcsKind::Academic => by.academic += 1,
    }
}
```

> 注意：sha256_hex 字段 spec 说 SHA-256；本 plan 用 FNV-1a hash 占位（"零新依赖"硬约束）。subagent 实装时如发现 chrono / serde_json 已有依赖链可白嫖 `digest`+`sha2` （它们 compile 进 binary 0 增量），改用真 SHA-256；如纯增量依赖，保留 FNV-1a 16 字符 hex 用作 envelope `sha256_hex`（**字段重命名为 `hash_hex` 更准确**，subagent 决策）。

- [ ] **Step 2: 接 commands/jwc/mod.rs**

Modify `src/commands/jwc/mod.rs`：加 `pub use ical::cmd_calendar;`

- [ ] **Step 3: cargo check（只 compile 不跑测，因为 handler 需要 Client 真链不在这测）**

```bash
cargo check --all-targets 2>&1 | tail -15
```
Expected: 编译过，0 warning（unused 不可放过）

- [ ] **Step 4: 既有测试不破**

```bash
cargo test --lib jwc::ical 2>&1 | tail -10
```
Expected: 23 测继续过

- [ ] **Step 5: fmt + clippy + 行数**

```bash
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && wc -l src/commands/jwc/ical/mod.rs
```
Expected: mod.rs ~160 < 200

- [ ] **Step 6: Commit**

```bash
git add src/commands/jwc/ical/mod.rs src/commands/jwc/mod.rs
git commit -m "feat(jwc): cmd_calendar handler + fail-soft + envelope mode (T5 step 6)"
```

---

## Task 7: CLI Calendar variant + dispatch + mockito 集成测

**Files:**
- Create: `src/cli/jwc/calendar_cli.rs` (~50 行)
- Modify: `src/cli/jwc/mod.rs` (加 `mod calendar_cli;` + `JwcSub::Calendar` variant + dispatch arm)
- Create (optional): `tests/jwc_calendar_integration.rs` (~120 行 mockito)

- [ ] **Step 1: 创建 calendar_cli.rs**

新建 `src/cli/jwc/calendar_cli.rs`：
```rust
//! `sjtu jwc calendar` clap variant 参数。
use clap::Args;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct CalendarArgs {
    /// 学年码（如 `2025`），默认按今天日期推。
    #[arg(long)]
    pub xnm: Option<String>,
    /// 学期码（3 / 12 / 16），默认按今天日期推。
    #[arg(long)]
    pub xqm: Option<String>,
    /// 输出文件路径（不传则 stdout）。
    #[arg(long)]
    pub to: Option<PathBuf>,
    /// 跳过学年校历那路。
    #[arg(long, default_value_t = false)]
    pub no_academic: bool,
    /// 跳过考试那路。
    #[arg(long, default_value_t = false)]
    pub no_exams: bool,
}
```

- [ ] **Step 2: 接 cli/jwc/mod.rs**

Modify `src/cli/jwc/mod.rs`：
1. 顶部加 `mod calendar_cli;` 和 `pub use calendar_cli::CalendarArgs;`
2. 在 `JwcSub` enum 加 variant：
```rust
    /// 校历 iCal 导出（个人课表 + 考试 + 学年校历）。
    Calendar(CalendarArgs),
```
3. 在 dispatch match 加 arm（参考 `JwcSub::Exams { ... } => jwc_cmds::cmd_exams(...)` 同 pattern）：
```rust
        JwcSub::Calendar(args) => {
            let client = crate::apps::jwc::Client::connect().await?;
            jwc_cmds::cmd_calendar(&client, args.xnm, args.xqm, args.to, args.no_academic, args.no_exams, fmt).await
        }
```

- [ ] **Step 3: cargo check + help 渲染**

```bash
cargo check --all-targets 2>&1 | tail -5
cargo run --release -- jwc calendar --help 2>&1 | head -20
```
Expected: 编译过；help 显示 5 个 arg

- [ ] **Step 4: 创建 mockito 集成测（可选，若行数预算紧可省）**

新建 `tests/jwc_calendar_integration.rs`：

> subagent 注意：写本测试需要把 jwc::Client 的 base URL 注入能力先做（T2.x mockito Client base URL injection 是已知遗留 debt）。本 plan **跳过 mockito 集成测**，靠 unit tests + T9 真机 smoke 覆盖。如 subagent 想做 mockito，**先开独立 PR 不阻塞 T5**。

- [ ] **Step 5: fmt + clippy + 行数**

```bash
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && wc -l src/cli/jwc/calendar_cli.rs src/cli/jwc/mod.rs
```
Expected: 0 diff / 0 warn / calendar_cli.rs ~30 / cli/jwc/mod.rs < 200

- [ ] **Step 6: Commit**

```bash
git add src/cli/jwc/calendar_cli.rs src/cli/jwc/mod.rs
git commit -m "feat(jwc): expose 'sjtu jwc calendar' CLI subcommand (T5 step 7)"
```

---

## Task 8: README / SKILL / lessons / todo 文档收尾

**Files:**
- Modify: `README.md`
- Modify: `SKILL.md`
- Modify: `tasks/lessons.md` (加 T5 lessons 章节)
- Modify: `tasks/todo.md` (加 2026-05-13 T5 完成行)

- [ ] **Step 1: README 已实装表加 calendar 行**

在 `## 已实装` 表里找到 jwc 行后面，加：
```markdown
| `sjtu jwc calendar` | RFC 5545 .ics 导出（个人课表 + 考试 + 学年校历）—— 4 端兼容（Google/Apple/Outlook/移动），CRLF + 75-octet 折行 + 内嵌 VTIMEZONE Asia/Shanghai；FNV-1a UID 幂等重导；零新依赖 |
```

- [ ] **Step 2: README 快速开始加 calendar 段**

在 `教务 GPA + 排名` 段后插：
````markdown
教务校历 iCal 导出（课表 + 考试 + 学年校历 三合一）：

```bash
sjtu jwc calendar > sjtu.ics                    # 默认学期 / stdout 写文件
sjtu jwc calendar --to sjtu.ics --json          # 落盘 + envelope 元数据
sjtu jwc calendar --xnm 2025 --xqm 12           # 指定学期
sjtu jwc calendar --no-academic --no-exams      # 只导课表
```

> 学年校历当前走 `tests/fixtures/jwc/academic_calendar_<xnm>_<xqm>.json` 静态文件，
> 每学期开学手工维护。后续若挖到 N 系列 endpoint 自动走 API。
````

- [ ] **Step 3: SKILL.md 加 `sjtu jwc calendar` 章节**

在 `### sjtu jwc exams` 章节后加：
```markdown
### sjtu jwc calendar
RFC 5545 .ics 导出，三路统一：个人课表 (N2151) + 考试 (N358105) + 学年校历 (fixture)。

**入参**：
- `--xnm <YYYY>` / `--xqm <3|12|16>`（默认推当前）
- `--to <file>`（不传则 stdout）
- `--no-academic` / `--no-exams`（跳过对应路）

**输出**：
- 默认 stdout 输出 .ics（管道友好 `> sjtu.ics`）
- `--to file` 写文件，stdout 改 envelope 元数据
- `--json` / `--yaml` 强制 envelope 模式

**Envelope shape**：`{ xnm, xqm, event_count, by_kind:{class,exam,academic}, sha256_hex, bytes, warnings[] }`

**fail-soft 语义**：三路任一失败其他两路仍出，`warnings[]` 填失败原因，exit 始终 0。

**兼容性硬保证**：CRLF 换行 + 75-octet 折行 + 内嵌 VTIMEZONE Asia/Shanghai + 稳定 FNV-1a UID（重复 import 幂等）+ X-WR-CALNAME / X-WR-TIMEZONE。`RRULE` 仅用 `FREQ=WEEKLY[;INTERVAL=2]`，不规则周（如军训 1-3 周 / 专题 5,7,9,11 周）explode 单次 VEVENT；默认无 VALARM（避免 Outlook 重复提醒）。
```

- [ ] **Step 4: lessons.md 加 2026-05-13 T5 章节**

在 lessons.md 末尾追加：
```markdown
---

## 2026-05-13 — T5 ical 4 端兼容硬规则与 hand-roll 决策

**触发情境**：T5 校历 iCal 导出 brainstorming 阶段 subagent 联网调研 4 端兼容性与 Rust 生态。

**关键发现 / 规则**：
1. **CRLF 强制**：LF 在 Apple Calendar 上被静默丢事件 —— 不是错误而是事件直接不出现，难诊
2. **VTIMEZONE 必须内嵌**：TZID-only 是 #1 时区 bug；4 端都不可靠地查 IANA tzdb 解 TZID 名
3. **Outlook BYWEEKNO 仍不安全（2026）**：MS-OXCICAL §2.1.3.2 明说 Outlook 只支持 RECUR 子集，不支持的整 VEVENT 被丢 —— 单/双周必走 `FREQ=WEEKLY;INTERVAL=2`，不规则周必 explode
4. **VALARM 默认禁**：Outlook 收到 VALARM 会发**重复**提醒（Outlook 自己也会按事件提醒）
5. **同领域 hand-roll 多于 crate**：`icalendar` 0.17 只省 ~30 行 plumbing 但 VTIMEZONE 仍要手卷或加二号依赖（vtimezones-rs），净收益不明 → A 路线零依赖
6. **75-octet 折行的 UTF-8 边界**：续行以 SP 起；多字节字符不可在 byte 边界切（folding 算 octet 但保 char）
7. **UID 不要密码学强度**：幂等去重用足，FNV-1a 64-bit hex 16 字符碰撞概率 2^-32 远超学期 ~300 事件 → 手卷 ~12 行省 sha1/sha2 依赖

**规则**：
- 任何对外文本协议（iCal / vCard / ics-like 格式）一律 CRLF 不议；测试单独覆盖换行字节
- 时区敏感格式优先内嵌 VTIMEZONE 而非 TZID-only
- recurrence 规则必须按"最严约束 client (Outlook)"设计，不按平均 client
- 优化路径优先考虑"工程量 vs 净收益"：crate 净收益 < 一两小时时坚持 hand-roll
```

- [ ] **Step 5: tasks/todo.md 加完成行**

在末尾加：
```markdown
| 2026-05-13 | T5 jwc 校历 iCal 导出 ✅ | spec → plan → 10 task 实装（T0 调研主对话亲跑 / T1-T8 subagent / T9 真机 4 端 smoke）；6 new files + 2 modify 全部 < 200 行；23+ unit test 全绿；CRLF + 75-octet 折行 + 内嵌 VTIMEZONE Asia/Shanghai + FNV-1a UID 幂等 + RRULE FREQ=WEEKLY[;INTERVAL=2] / 不规则周 explode；fail-soft warnings[] + envelope/stdout 双模式；零新依赖；CP-Cal-1..4 真机 4 端各 import 1 次 + 重复 import 无重复事件 | sub_session staleness CAS 侧 `login_slogin.html` final_url 检测 + 自动 invalidate 仍是 T2.x 遗留债；mockito Client base URL 注入 (T2.x) 跳过 |
```

- [ ] **Step 6: Commit**

```bash
git add README.md SKILL.md tasks/lessons.md tasks/todo.md
git commit -m "docs(jwc): T5 ical export — README + SKILL + lessons + todo 收尾"
```

---

## Task 9: T9 主对话 — CP-Cal-1..4 真机 4 端 smoke

> **主对话亲跑**（subagent 没 4 端日历账号；用户协作）。

**Pre**: T1-T8 全完成 / cargo build --release 通过 / 用户登过 i.sjtu

- [ ] **Step 1: 真机产 .ics**

```bash
./target/release/sjtu.exe jwc calendar --xnm 2025 --xqm 12 --to sjtu_2025_12.ics --json
```

期望 envelope：
- `event_count > 50` (一学期 10-20 门课 × 18 周 + 5-10 考试 + 6 节假日 ≈ 60-100 events)
- `by_kind.class > 0` / `by_kind.exam > 0` / `by_kind.academic >= 6`
- `bytes > 5000` < 100000
- `warnings: []` 或仅 fixture 提示

记 sha256_hex 作为 baseline。

- [ ] **Step 2: CP-Cal-1 Google Calendar Web**

用户操作：
1. https://calendar.google.com/ → 设置 → 添加日历 → 从文件导入 → 选 sjtu_2025_12.ics → 任一日历
2. 在 web 上看一周课表：
   - 课程时间是 GMT+8（不是 UTC）
   - 周复发课 (FREQ=WEEKLY) 全 18 周连续出
   - 单/双周课只在对应周出
   - 不规则课只在指定周出
3. 不报错事件数

- [ ] **Step 3: CP-Cal-2 Apple Calendar (macOS / iOS)**

用户操作 macOS：双击 sjtu_2025_12.ics → 选目标日历 → 导入
或 iOS：邮件附件 → 添加到日历

期望：同 Step 2 验证项 + 无静默丢事件（用 grep 数 envelope event_count vs 实际客户端事件数）

- [ ] **Step 4: CP-Cal-3 Outlook Web (M365)**

用户操作：outlook.live.com → 添加日历 → 从文件 → 选 sjtu_2025_12.ics

期望：
- 单/双周课正确显示（INTERVAL=2 兼容）
- 不规则课 explode 后正确显示
- VEVENT 不被丢（数量与 envelope 匹配）

- [ ] **Step 5: CP-Cal-4 移动本地导入（iOS / Android 任一）**

用户操作：把 .ics 邮件给自己 → 在手机邮箱客户端打开附件 → 选 "加入日历"

期望：基本课程时间 / 时区显示正确（最低端验证）

- [ ] **Step 6: 重复 import 幂等验证（任选一端）**

再 import 一次同 .ics → 不应复制事件（UID 稳定）。

- [ ] **Step 7: 真机 smoke 结果文档化**

在 `tasks/todo.md` 已加的 T5 完成行的"瑕疵"列里如有 → 补 4 端实测发现的差异（如某端 explode 显示异常 / 时区显示有偏 / 等）。如全过 → 同行的"备注"段加"4 端 import 全过 CP-Cal-1..4 ✓"。

- [ ] **Step 8: 最终 commit + push**

```bash
git add tasks/todo.md
git commit -m "chore(s5): T5 CP-Cal-1..4 真机 4 端 smoke 全过"
git push origin main
```

---

## 自审 (writing-plans skill 要求)

### 1. Spec coverage
- spec §1 现状 / 约束 → Task 0-8 全覆盖
- spec §2 G1-G7 / NG1-NG6 → Task 1-9 全覆盖；NG 用"不做"映射明确
- spec §3 6 new + 2 modify 文件 → Task 1-7 一对一映射，Task 8 文档
- spec §4 CLI 接面 → Task 7 calendar_cli.rs
- spec §5 数据流 → Task 4 events.rs + Task 6 handler
- spec §6 RFC 5545 硬规则 9 条 → Task 2 writer/vtimezone (规则 1-3,5-7,10) + Task 4 events (规则 4)
- spec §7 recurrence 4 类 → Task 3
- spec §8 fail-soft → Task 6 handler
- spec §9 测试三档 → Task 1-5 unit + Task 9 真机；mockito 集成测延后（已记 debt）
- spec §10 OQ1-5 → OQ1 Task 0；OQ2 plan 决策（手卷 FNV-1a）；OQ3 Task 2 单测覆盖；OQ4 Task 8 文档；OQ5 Task 6
- spec §11 task 拆分预估 → 1-9 task 编号一致
- spec §12 真机通过标准 → Task 9 step 2-6 一对一映射

### 2. Placeholder scan
- 全文无 "TBD" / "TODO" / "implement later"
- 每个代码 step 都给具体 code 块（不是描述）
- mockito 集成测显式标 "跳过 + 已知 debt"，非 TODO
- KbItem.cdmc / Exam.cdmc 字段名 subagent 阅源决定，给了 fallback 行为（缺则 None）

### 3. Type consistency
- `Recurrence` enum (3 variant) 在 Task 3 定义，Task 4/6 引用一致
- `IcsEvent` struct 在 Task 4 定义，Task 6 引用一致
- `VEventFields` struct 在 Task 2 定义，Task 6 引用一致
- `CalendarData` / `ByKind` 在 Task 6 内自包含
- `fnv1a_64` 函数签名 `fn(s: &str) -> String` 整 plan 一致
- `term_first_monday` 签名 `fn(xqkssj: &str) -> Option<NaiveDate>` Task 4 定义、Task 6 使用一致
- `load_from_fixture` 签名 `fn(xnm: &str, xqm: &str) -> Result<AcademicCalendar>` Task 5 定义、Task 6 使用一致

无类型 / 命名 drift。Plan 可执行。
