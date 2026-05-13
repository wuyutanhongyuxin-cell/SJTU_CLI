# T5 jwc 校历 iCal 导出 — Design Spec

> **状态**：Draft → 待用户过审 → 转 writing-plans
> **日期**：2026-05-13
> **范围**：新增 `sjtu jwc calendar` 子命令，把当前学期的"个人课表 + 考试 + 学年校历"三路统一输出为 RFC 5545 合规 .ics，兼容 Google Calendar / Apple Calendar (macOS/iOS) / Outlook / 移动本地导入 4 端
> **复用基线**：N2151 整学期课表 + period_clock (T1 已实装) / N358105 考试 (S3f 已实装) / jwc CAS / oauth2 staleness fix (今日 11e1917) / Envelope / chrono

---

## 1. 背景与现状

### 1.1 现状基线

T1 + T2 完成后 jwc 子系统已具备：

| 层 | 文件 | 用途 |
|----|------|------|
| Model | `src/apps/jwc/models/schedule.rs` `KbItem` | 课表条目（含 `zcd` 周次字符串） |
| Model | `src/apps/jwc/models/exam.rs` `Exam` | 考试条目（已有 `kssj` 起止时刻） |
| API | `src/apps/jwc/api/schedule.rs` | N2151 整学期 / `schedule_by_week` 周次 |
| API | `src/apps/jwc/api/exams.rs` | N358105 考试 |
| Util | `src/apps/jwc/period_clock.rs` | 1-13 节起止 lookup + bitmask |
| Util | `src/apps/jwc/api/term.rs::infer_current_xnxq` | 推默认学年学期 |
| CLI | `src/cli/jwc/mod.rs` | grades / schedule / today / week / next / gpa / gpa-by-semester / exams |

**缺口**：没有 .ics 导出 / 没有学年校历 endpoint / 没有 recurrence rule writer。

### 1.2 关键技术约束（继承）

- **i.sjtu 硬红线**：只读；不提交任何表单 / 不点写按钮
- **不引入新依赖**：复用 chrono / serde / sha1 (std no，但已通过 sha1 等价 lib？后述决策)；本期不动 Cargo.toml
- **行数硬限**：单源文件 200 行 / 单测文件 300 行
- **Envelope additive**：仅新增子命令，不动既有命令的 envelope shape

### 1.3 研究综合（决策依据）

联网调研 4 点 (subagent 报告 2026-05-13)：

1. **Rust 生态**：`icalendar` 0.17.x 主流但 VTIMEZONE 仍要手卷（或再依赖 `vtimezones-rs`），用 crate 只省 ~30 行 plumbing 但仍要写 VTIMEZONE + 测 CRLF/folding；净收益不明
2. **同领域实践**：教务 → iCal 项目 overwhelmingly 手卷；常见 bug 是 LF vs CRLF 静默丢事件（Apple）/ 缺 75-octet 折行 / VTIMEZONE 格式错
3. **Outlook 2026 现状**：`BYWEEKNO` **仍不安全**（MS-STANOICAL §3.8.5.3：Outlook 只支持 RECUR 子集，不支持的整个 VEVENT 被丢弃）。不规则周必须 explode VEVENT，不能依赖 RRULE+EXDATE
4. **4 端最小公约数**：CRLF + 75-octet 折行 + 稳定 UID + **内嵌 VTIMEZONE Asia/Shanghai**（TZID-only 是 #1 时区 bug）+ `X-WR-CALNAME` / `X-WR-TIMEZONE` (Google 读其他忽略) + 默认不要 VALARM（Outlook 会发重复提醒）

→ Approach A 胜出：**单命令 + 手卷 ~80-120 行 RFC 5545 输出，零新依赖**。

---

## 2. Goals / Non-Goals

### Goals

- G1 — 单学期"课表 + 考试 + 学年校历"统一 .ics，4 端 import 通过
- G2 — `sjtu jwc calendar` 单子命令；stdout 默认 / `--to file` 落盘 / `--json` 元数据 envelope
- G3 — RFC 5545 合规硬规则（CRLF / 75-octet 折行 / 内嵌 VTIMEZONE / 稳定 UID）
- G4 — recurrence classifier 区分 4 类周次模式，规则的走 RRULE 不规则的 explode
- G5 — fail-soft：三路任一失败其他两路仍出，envelope warnings[] 提示；exit 始终 0（envelope 模式）
- G6 — 真机 smoke：4 端各 import 一次，验证零事件丢失 / 时区正确 / 重复导入幂等
- G7 — 文档收尾（README / SKILL / tasks/todo / tasks/lessons）

### Non-Goals

- **NG1**：不做日程提醒（VALARM）—— Outlook 重复提醒坑，默认禁用；用户可在客户端自加
- **NG2**：不做跨学期合一 .ics —— 单学期 / 单文件
- **NG3**：不用 `BYWEEKNO` —— Outlook drop event 风险
- **NG4**：不接其他 SP 写日历事件回 SJTU（硬红线）
- **NG5**：不做 .ics 解析 / 反向导入（write-only）
- **NG6**：不本期做 V2 增强（订阅式 webcal:// URL / 命令调度 / 多语言 SUMMARY）

---

## 3. 文件布局

6 新文件 + 2 modify，全部 < 200 行：

```
src/
├── apps/jwc/
│   ├── api/
│   │   ├── calendar.rs              # +60 行：学年校历 endpoint（T0 调研定 / fallback fixture loader）
│   │   └── mod.rs                   # +1 行：pub mod calendar
│   └── models/
│       ├── calendar.rs              # +50 行：AcademicEvent struct
│       └── mod.rs                   # +1 行：pub mod calendar
├── commands/jwc/
│   ├── ical/
│   │   ├── mod.rs                   # +30 行：cmd_calendar handler 入口
│   │   ├── writer.rs                # +80 行：VCALENDAR/VEVENT writer + CRLF + 75-octet 折行
│   │   ├── vtimezone.rs             # +30 行：Asia/Shanghai VTIMEZONE 硬编码块
│   │   ├── events.rs                # +60 行：KbItem / Exam / AcademicEvent → IcsEvent 统一
│   │   └── recurrence.rs            # +50 行：zcd 周次字符串 → 4 类决策
│   └── mod.rs                       # +2 行：pub mod ical; pub use ical::cmd_calendar
└── cli/jwc/
    ├── calendar_cli.rs              # +50 行：Calendar variant + dispatch arm
    └── mod.rs                       # +5 行：JwcSub::Calendar variant
```

测试文件：
```
src/commands/jwc/ical/tests.rs        # ~200 行：writer / recurrence / vtimezone 单测
tests/fixtures/jwc/
├── academic_calendar_2024_12.json    # T0 调研产 / 人工灌
└── calendar_smoke.ics                # 真机产 baseline（脱敏）
```

---

## 4. CLI 接面

```bash
sjtu jwc calendar                                # stdout .ics（管道 > sjtu.ics）
sjtu jwc calendar --to file.ics                  # 写文件
sjtu jwc calendar --xnm 2025 --xqm 12            # 指定学期（默认推当前）
sjtu jwc calendar --no-academic                  # 跳过学年校历那路
sjtu jwc calendar --no-exams                     # 跳过考试那路
sjtu jwc calendar --json                         # 不输 .ics，回 Envelope 元数据
```

`--json` 输出形态：
```yaml
ok: true
data:
  xnm: 2025
  xqm: 12
  event_count: 234     # 总 VEVENT 数
  by_kind: {class: 198, exam: 12, academic: 24}
  sha256: 'abc123...'  # ics bytes 全文校验和
  bytes: 18432
  warnings: []         # ['学年校历 endpoint 未找到，已用 fixture 退化']
error: null
schema_version: '1'
```

`--to file` 时同时输出 ics 文件 + stdout 上面这个 envelope。

---

## 5. 数据流

```
CLI args
  ↓
推默认 xnm/xqm (jwc::api::term::infer_current_xnxq)
  ↓
并行 3 fetch:
  ├── client.schedule(xnm, xqm)            → Vec<KbItem>   (N2151)
  ├── client.exams(xnm, xqm)                → Vec<Exam>     (N358105)
  └── client.academic_calendar(xnm, xqm)    → Vec<AcademicEvent> (T0 endpoint / fixture)
  ↓
events::unify(KbItem[], Exam[], AcademicEvent[]) → Vec<IcsEvent>
  ↓
recurrence::classify(IcsEvent) → Vec<IcsEventEmit>  (展开不规则周)
  ↓
writer::emit(events) → Vec<u8>  (CRLF + 75-octet 折行 + VCALENDAR)
  ↓
stdout / --to file / --json envelope
```

`IcsEvent` unified 模型（events.rs 内部）：

```rust
struct IcsEvent {
    uid_seed: String,         // 用于 sha1 UID 生成
    summary: String,          // 课名 / 考试名 / 校历事件
    description: Option<String>,
    location: Option<String>, // 教室 / 考场
    kind: IcsKind,            // Class | Exam | Academic
    dtstart: DateTime<Tz>,    // 含 Asia/Shanghai tzid
    dtend: DateTime<Tz>,
    recurrence: Option<Recurrence>, // None=单次, Some=RRULE
}
```

---

## 6. RFC 5545 关键约束（硬规则）

| 规则 | 实施位置 | 备注 |
|------|----------|------|
| **CRLF 换行** | writer.rs `emit_line()` | Apple Calendar 见 LF 静默丢事件 |
| **75-octet 折行** | writer.rs `fold_line()` | 续行以 SP 开头 |
| **内嵌 VTIMEZONE** | vtimezone.rs 硬编码 Asia/Shanghai | TZID-only 不嵌是 #1 时区 bug |
| **稳定 UID** | events.rs `make_uid()` | `sha1(xnm+xqm+course_id+jc+xqj+zc)[:16] @ sjtu-cli` |
| **DTSTART;TZID=Asia/Shanghai** | writer.rs | 不能用 Z（UTC）+ offset |
| **X-WR-CALNAME / X-WR-TIMEZONE** | writer.rs header | Google 读其他忽略，无害 |
| **DTSTAMP = UID 生成时刻** | events.rs | 重复导入幂等关键 |
| **PRODID = sjtu-cli/X.Y.Z** | writer.rs header | RFC 5545 必填 |
| **不用 BYWEEKNO** | recurrence.rs | Outlook drop event |
| **VALARM 默认禁用** | writer.rs | Outlook 重复提醒；用户自加 |

VTIMEZONE 静态块（Asia/Shanghai 中国无 DST，全年 UTC+8）：

```ics
BEGIN:VTIMEZONE
TZID:Asia/Shanghai
BEGIN:STANDARD
DTSTART:19890101T000000
TZOFFSETFROM:+0800
TZOFFSETTO:+0800
TZNAME:CST
END:STANDARD
END:VTIMEZONE
```

---

## 7. recurrence classifier

输入：N2151 KbItem 的 `zcd` 周次字符串（如 `"1-18周"` / `"1-18周(单)"` / `"3,5,7,9周"`）

输出：4 种决策

| 类别 | 触发条件 | 实施 | 示例 |
|------|----------|------|------|
| **A. 全学期连续** | `"1-N周"` 且无 单/双 修饰 | `RRULE FREQ=WEEKLY;COUNT=N` | "1-18周" → COUNT=18 |
| **B. 规则单/双周** | `"1-N周(单)"` 或 `"1-N周(双)"` | `RRULE FREQ=WEEKLY;INTERVAL=2;COUNT=⌈N/2⌉` | "1-18周(单)" → INTERVAL=2 COUNT=9 起 1 |
| **C. 不规则离散** | `"a,b,c,...周"` 任意 | explode 每周一个 VEVENT，UID 后缀 `_w<zc>` | "3,5,7,11周" → 4 个 VEVENT |
| **D. 短开范围** | `"a-b周"` 且 `b-a+1 ≤ 3` | explode（不值得 RRULE） | "1-3周" → 3 个 VEVENT（军训类） |

考试 / 学年校历事件总是 `Recurrence::None` 单次。

---

## 8. 错误处理 fail-soft（沿用 gpa-by-semester 模式）

三 fetch 并行，任一失败：
- 其他两路仍正常出
- envelope `warnings[]` 加一条 `"<kind> 调用失败: <reason>"`
- exit code 0（envelope 模式）

特殊：学年校历 endpoint 调研失败 / 当前学期 fixture 缺失：
- stderr warn：`"未找到 2025-12 学期学年校历 fixture，仅输出课表+考试"`
- envelope warnings[] 同步
- exit 0

writer 阶段异常（比如 IcsEvent 字段缺）→ 该事件 skip，warnings[] 计数；其他事件继续。

CLI 参数错（如 --to 路径不可写）→ 直接 exit 1（envelope error）。

---

## 9. 测试策略

### 9.1 单测（src/commands/jwc/ical/tests.rs ≤ 200 行）

- `writer::fold_line` —— 75-octet 边界 + 多字节 UTF-8 不切字
- `writer::emit_line` —— CRLF 字节
- `vtimezone::block` —— bytes 比对预期常量
- `recurrence::classify` —— 4 类 case × 各 1-2 边界
- `events::make_uid` —— 同输入同 UID（幂等）
- `events::unify` —— 三路合一去重

### 9.2 集成测（tests/jwc_calendar_integration.rs，可选 ≤ 200 行）

- mockito 模拟 N2151 + N358105 + 学年校历，端到端 .ics 文本字节比对 baseline
- 用 `tests/fixtures/jwc/calendar_smoke.ics` 当 baseline

### 9.3 真机 smoke checklist（CP-Cal-1..4）

- CP-Cal-1：`sjtu jwc calendar --xnm 2025 --xqm 12 --to /tmp/sjtu.ics` 落盘 + sha256 record
- CP-Cal-2：Google Calendar Web import → 事件数 / 时区 / 一周课表显示
- CP-Cal-3：Apple Calendar (macOS) import → 同上
- CP-Cal-4：Outlook Web import → 同上 + 单/双周事件正确
- 重复 import：UID 稳定 → 不复制事件

---

## 10. Open Questions

| ID | 问题 | 决策时点 |
|----|------|----------|
| OQ1 | T0 调研是否能挖到学年校历 N 系列 endpoint | T0 调研阶段，挖不到走 fixture |
| OQ2 | sha1 是否需要新依赖？std 没 sha1 | T1 设计阶段定（候选：`sha1 = "0.10"` 小依赖 / 复用现有 crypto 链 / 自卷简化 hash） |
| OQ3 | CRLF 折行边界 75 octet 是否考虑 UTF-8 多字节字符的字节预算 | writer.rs 实装时按 octet 不按 char，多字节字符要整体保留不切；test 一定覆盖 |
| OQ4 | 学年校历 fixture 的人工维护流程 | 文档说明：每学期开学 user 贴权威源文本，agent 转 JSON 落 tests/fixtures/jwc/academic_calendar_<xn>_<xq>.json |
| OQ5 | --to 时 sha256 校验和写文件还是只回 envelope | 决策：envelope 给 hash 字符串，文件只是 raw ics |

---

## 11. 任务拆分（plan 阶段会细化）

预估 8-10 task：

- **T0 主对话亲跑**：chrome-devtools 半自动调研学年校历 endpoint（OQ1）+ 当前学期 fixture 落盘
- **T1**：models/calendar.rs `AcademicEvent` struct + serde 测
- **T2**：api/calendar.rs endpoint 或 fixture loader（双轨）
- **T3**：commands/jwc/ical/writer.rs + vtimezone.rs + 8-10 个 writer 单测
- **T4**：commands/jwc/ical/recurrence.rs + 4 类 classifier 单测
- **T5**：commands/jwc/ical/events.rs unify + UID 算法 + 单测
- **T6**：commands/jwc/ical/mod.rs cmd_calendar handler + fail-soft + envelope 模式
- **T7**：CLI/jwc/calendar_cli.rs Calendar variant + dispatch
- **T8**：mockito 集成测 + baseline ics 文件
- **T9 主对话亲跑**：CP-Cal-1..4 真机 smoke 4 端 import + 重复导入幂等
- **T10**：文档收尾（README / SKILL / lessons / todo）

---

## 12. 真机 smoke 通过标准

- 4 端各自 import 一次 .ics 后：
  - 事件数 == envelope event_count
  - 任取 3 节常规课，DTSTART 显示时区为 GMT+8 / CST
  - 任取 1 节单/双周课，RRULE INTERVAL=2 显示对（不应有缺失周也不应有多余周）
  - 任取 1 节不规则课（如军训），explode 出来的 N 个事件全到位
  - 任取 1 个考试，DTSTART 显示在指定考场时间
- 同 .ics 文件二次 import：事件不重复（UID 幂等）
- 课表 + 考试 + 学年校历 三类事件在客户端着色 / category 不冲突（X-CATEGORY 可选）
