# jwc 课表衍生命令设计：today / week / next

> 日期：2026-05-12
> 范围：T1（jwc 纵向第 1 步）
> 前置调研：`tasks/isjtu_investigation.md` §1.4（headers）/ §2.2（N2151，已实装）/ §2.7（N2154）
> 红线遵守：`CLAUDE.md` "i.sjtu / 交我办 硬红线"（只读 / 不提交表单 / 不变更状态）
> 关键决策：参见本文档 §13（"决策记录"）

---

## 1. 一句话

在已实装的 N2151 学年学期课表之上，新增 N2154 周次课表端点 + 三个衍生 CLI 命令（`sjtu jwc today` / `week` / `next`），用 ZF 返回的 `oldzc` 16-bit 周次 bitmask 精确过滤本周课，并提供节次→时刻映射 + 可选 `--grid` 表格输出，让"我今天有什么课"成为一行命令的事。

## 2. Goal / Non-Goal

### Goal

| # | 能力 | 验收 |
|---|---|---|
| G1 | `sjtu jwc today` 输出今天剩余的课（含时刻、教室、教师） | 真机 2026-05-12（周二）能列出本周二课，过期节次自动过滤 |
| G2 | `sjtu jwc week [--zs N]` 输出指定周（默认当前周）整周课表 | `--zs 14` 等价"第 14 教学周"；不传 `--zs` 时自动反推 |
| G3 | `sjtu jwc next [--within N] [--limit K]` 输出接下来 N 天内 K 节课 | `--within 1` = 今天剩余；`--within 31` = 未来一个月 |
| G4 | `--grid` 标志切换到 7×N 表格输出（comfy-table 渲染） | TTY 下肉眼可读，非 TTY 下走默认列表（保持 YAML/JSON 兼容） |
| G5 | "今天是第几周"反推算法 + 24h cache | 不需要用户手动传 `--zs`；首次调用 < 1.5s，后续 < 0.3s |
| G6 | 节次 → 起止时刻映射 | 第 3 节 = "10:00-10:45"；优先 ZF 字典端点动态拉，调研失败用 fallback 硬编码常量表 |

### Non-Goal（明确不做）

- ❌ **GPA 重算 / 学期均分**：T2 范围，本 spec 不涉及
- ❌ **课程冲突检测 / 提醒**：v0.3+
- ❌ **iCal / 校历 ics 导出**：T5 单独 spec
- ❌ **跨学期 schedule**：`--within` 仅在同学期内有效，跨学期返回 hint
- ❌ **N2151 路径改造**：保留现有 `cmd_schedule`（学年学期全量视图）100% 不动
- ❌ **修改 grades / gpa / exams handler**：本 spec 仅 additive

## 3. 决策证据 / 已知约束

| 约束源 | 内容 |
|---|---|
| `isjtu_investigation.md` §2.7 | N2154 端点 `/kbcx/xskbcxMobile_cxXsKb.html`，form `xnm/xqm/zs(1..18)/kblx=1/doType=app/xh=`，返回 `rqazcList`（每天 ISO 日期）+ `kbList`（含 `oldzc` 16-bit / `oldjc` bitmask） |
| `isjtu_investigation.md` §1.4 | 必带 `X-Requested-With: XMLHttpRequest` / `Origin` / `Referer` / 真 UA，CSRF token 已由 `post_form_json` 注入 |
| `CLAUDE.md` i.sjtu 红线 | N2154 是 read-only 查询路径（无 form.submit / 无状态变更），符合红线；调研期 chrome-devtools 仅 take_snapshot / evaluate_script（只读 JS） |
| `CLAUDE.md` §"上下文管理规范" | 单源码文件 ≤ 200 行，触发本 spec 的 3 处文件拆分 |
| 现有 `src/config.rs` | 用 `directories::ProjectDirs` 解析跨平台路径；cache 应走 `cache_dir()` 而非 `config_dir()` |

## 4. 用户故事 / 命令清单

```bash
# G1: 今天剩余的课
sjtu jwc today
sjtu jwc today --yaml
sjtu jwc today --grid                  # 单日 1×N 表格

# G2: 整周课表
sjtu jwc week                          # 自动反推当前周
sjtu jwc week --zs 14                  # 显式指定第 14 教学周
sjtu jwc week --grid                   # 7×N 表格输出

# G3: 接下来若干节课
sjtu jwc next                          # 默认 --within 1 --limit 5
sjtu jwc next --within 7 --limit 20    # 本周剩余前 20 节
sjtu jwc next --within 31              # 整月（最多 5 周次 N2154 调用）

# 共用参数（继承自 N2151 cmd_schedule）
sjtu jwc today --xnm 2025 --xqm 12     # 显式指定学年学期，跳过反推
```

## 5. Architecture

```
                    ┌─────────────────────────────────┐
                    │  现有 N2151 (学年学期全量)        │
                    │  schedule(xnm, xqm)             │  ← 不动
                    └─────────────────────────────────┘
                                    │
                                    │
  ┌─────────────────────────────────┼─────────────────────────────────┐
  │  新增 N2154 (周次)              │                                 │
  │                                 ▼                                 │
  │  schedule_by_week(xnm, xqm, zs) ────┐                             │
  │                                     │                             │
  │  infer_current_week(xnm, xqm)       │  ← 调一次 zs=1 反推今天周次 │
  │    └ 24h cache (~/.cache/...)        │                             │
  │                                     ▼                             │
  │  period_clock::lookup(jc) → (start, end)                          │
  │    └ T0 调研 ZF 字典 + fallback 硬编码                            │
  └───────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
              ┌────────────────────────────────────────┐
              │  commands::jwc::schedule_handlers      │
              │  cmd_today  /  cmd_week  /  cmd_next   │
              └────────────────────────────────────────┘
                                    │
                                    ▼
              ┌────────────────────────────────────────┐
              │  Envelope (additive) → YAML / JSON     │
              │  或 --grid → output::grid              │
              └────────────────────────────────────────┘
```

**策略**：纯增量 / 0 改造。N2151 路径完全不动。N2154 是平行通道。所有衍生命令都基于 N2154。

## 6. Components（文件清单）

### 6.1 新建文件

| 文件 | 用途 | 预估行数 |
|---|---|---|
| `src/apps/jwc/period_clock.rs` | 节次 → 起止时刻常量表（硬编码 1-13 节） + ZF 字典端点动态拉的 helper（T0 调研后填） | ~120 |
| `src/commands/jwc/schedule_handlers.rs` | `cmd_today` / `cmd_week` / `cmd_next` + Envelope payload 拼装（不含 grid 渲染） | ~180 |
| `src/cli/jwc/mod.rs` | 替换当前 `src/cli/jwc.rs`（拆模块入口） | ~155（现 jwc.rs 内容） |
| `src/cli/jwc/schedule_cli.rs` | `Today / Week / Next` clap subcommand 定义 + `--grid` / `--within` / `--limit` / `--zs` 参数 | ~95 |
| `src/output/grid.rs` | comfy-table 包装：`render_grid_day(items)` + `render_grid_week(items, rqazc_list)` | ~90 |
| `src/config.rs` 加 `cache_dir()` + `jwc_week_cache_path()` + `ensure_cache_dir()` | +25 行（不超 200 限） | - |
| `tests/fixtures/jwc/n2154_week_zs1.json` | 真机抓 + 脱敏的 N2154 响应（zs=1 用于反推） | 数据 |
| `tests/fixtures/jwc/n2154_week_zs14.json` | 真机抓 + 脱敏的 N2154 响应（zs=14 用于过滤） | 数据 |

### 6.2 修改文件

| 文件 | 改动 | 行数变化 |
|---|---|---|
| `src/apps/jwc/api/schedule.rs` | 加 `schedule_by_week(xnm, xqm, zs)` + `infer_current_week(xnm, xqm)` + cache I/O helper | 42 → ~135 |
| `src/apps/jwc/models/schedule.rs` | `KbItem` 加 `old_zc: Option<u32>` + `old_jc: Option<u32>`；新建 `RqAzc { rq, xqj }` 结构；`Schedule` 加 `rqazc_list: Vec<RqAzc>` | 106 → ~140 |
| `src/apps/jwc/mod.rs` | re-export 新增类型 `RqAzc` / `period_clock` | +5 |
| `src/commands/jwc/handlers.rs` | 不动（保留 4 现有 handler） | 126（不变） |
| `src/commands/jwc/data.rs` | 加 `TodayData` / `WeekData` / `NextData` envelope payload struct | +60 |
| `src/commands/jwc/mod.rs` | re-export `schedule_handlers::{cmd_today, cmd_week, cmd_next}` | +3 |
| `src/cli/jwc.rs` | **删除**（内容拆到 `src/cli/jwc/mod.rs` + `schedule_cli.rs`） | 151 → 0 |
| `src/cli/mod.rs` | `pub mod jwc;` 不变（目录入口自动生效） | 0 |
| `Cargo.toml` | 加 `comfy-table = "~7.1"` 依赖（锁 7.1.x，MSRV 1.64 兼容；**7.2 MSRV 1.85 超项目 rust-version 1.75，禁用**）| +1 |
| `README.md` | 命令表新增 3 行（today / week / next） | +3 |
| `tasks/todo.md` | T1 标完成 + 列遗留 | - |
| `tasks/lessons.md` | 加 N2154 bitmask / 周次反推 / cache 分离教训 | - |

### 6.3 文件 200 行硬限合规性

| 拆分点 | 拆前 | 拆后 |
|---|---|---|
| `src/cli/jwc.rs`（151）+ Today/Week/Next 定义（+60） = ~211 | 超 | `src/cli/jwc/mod.rs`（~155，re-export + 4 现有 sub）+ `schedule_cli.rs`（~95，新 3 sub）|
| `src/commands/jwc/handlers.rs`（126）+ 3 新 handler（+120）= ~246 | 超 | `handlers.rs`（126 不动）+ `schedule_handlers.rs`（~180）|
| `src/apps/jwc/api/schedule.rs`（42）+ N2154 + cache（+90）= ~135 | OK | 同文件，不拆 |

**模块目录总和（apps/jwc/、commands/jwc/、cli/jwc/）** 均 < 2000 行硬限。

## 7. Data Flow / Key Algorithms

### 7.1 当前周次反推

```
[cmd_today / cmd_week (no --zs) / cmd_next]
                │
                ▼
  read ~/.cache/sjtu-cli/jwc_week_cache.json
                │
                ├── hit & fetched_at < 24h ago ──→ cw = cache["{xnm}-{xqm}"]
                │
                └── miss / 过期
                        │
                        ▼
                schedule_by_week(xnm, xqm, zs=1)
                        │
                        ▼
                rqazc_list[0].rq = "2025-09-08"  (第 1 周 周一 ISO)
                        │
                        ▼
                today = chrono::Local::today().naive_local()
                delta = (today - week1_monday).num_days()
                cw = (delta / 7 + 1) as u8
                        │
                        ├── cw < 1   → cw = 1   + envelope.hint = "学期未开始"
                        ├── cw > 18  → cw = 18  + envelope.hint = "学期已结束 / 假期"
                        └── 1..=18   → OK
                        │
                        ▼
                write cache[{xnm}-{xqm}] = { week: cw, fetched_at: now() }
```

**cache key 设计**：`{xnm}-{xqm}` 复合键，保证跨学期切换不误判。`xnm` / `xqm` 为空时（用户未指定）→ 调一次 N2154 zs=1 时 ZF 会用 default，从返回的 rqazc_list 反推时取 ZF 实际服务的学年学期（响应里会有 echo 字段或可推断）。MVP 简化：若 xnm/xqm 为空 → cache key 用 `"current"`，TTL 强制缩到 1h（更频繁刷新避免学期换季时误判）。

### 7.2 oldzc / oldjc bitmask

N2154 返回的 `oldzc` 真机是**数字字符串**（例 `"65535"`，T0 fixture `tests/fixtures/jwc/n2154_week_zs1.json` 印证），Rust 端用 `deserialize_with` 反序列化为 `u32` 后做 bitmask 计算。位 0 = 第 1 周，位 N-1 = 第 N 周；ZF 通常 ≤ 18 教学周，但 32-bit 整型可容纳 32 周。`oldjc` 同理（位 0 = 第 1 节，真机也是 string）。`rqazcList[*].xqj` 真机是 number (1..7)，与 `kbList[*].xqj`（string "1".."7"）不一致 — 模型里 `RqAzc.xqj: Option<u8>`，`KbItem.xqj: Option<String>` 分别处理。

```rust
pub fn is_in_week(old_zc: u32, week: u8) -> bool {
    if week == 0 || week > 32 { return false; }
    (old_zc >> (week - 1)) & 1 == 1
}

pub fn jc_positions(old_jc: u32) -> Vec<u8> {
    (0u32..32)
        .filter(|i| (old_jc >> i) & 1 == 1)
        .map(|i| (i + 1) as u8)
        .collect()
}
```

**Fallback**：当 N2154 响应里 `oldzc` 缺失或 0（极端情况），改读 `kbItem.jcor`（形如 "3-4"）+ `kbItem.zcd`（形如 "1-18周"）字符串解析，envelope 附 `meta.bitmask_fallback: true` 标记降级。

### 7.3 节次 → 时刻映射

```rust
// src/apps/jwc/period_clock.rs
pub fn lookup(jc: u8) -> Option<(NaiveTime, NaiveTime)> {
    DEFAULT_TABLE.get(jc as usize - 1).copied()
}

// SJTU 闵行 / 徐汇主区表 fallback（T0 调研失败时启用）
const DEFAULT_TABLE: [(NaiveTime, NaiveTime); 13] = [
    // 节次 1..13
    // 待 T0 调研填实际时刻；下表是调研前的占位值，T0 完成后替换。
    (NaiveTime::from_hms_opt(8, 0, 0).unwrap(),  NaiveTime::from_hms_opt(8, 45, 0).unwrap()),
    // ...
];
```

**T0 调研任务**（plan 第 0 步主对话亲跑）：
- 用 chrome-devtools `take_snapshot` / `evaluate_script`（只读 JS）扫 i.sjtu.edu.cn 的字典端点
- 候选 URL：`/xtgl/zdpz_cxZdpzList.html?gnmkdm=N2151&doType=query`（参考 §1.x ZF 公共字典模式）
- 若找到：填 `period_clock.rs` 的 ANIMATE_TABLE + 提供 fetch helper（保留 fallback）
- 若失败：用 SJTU 教务处公开的节次时刻表（**人工录入**，附信源链接 in code 注释）

### 7.4 next within N 算法

```
cmd_next(within=N, limit=K)
        │
        ▼
infer_current_week → cw
        │
        ▼
weeks_to_fetch = ceil((today_remaining_days + N) / 7)
                  最小 1，最大 5（覆盖 31 天 = 4.43 周 → 取 5）
        │
        ▼
并行调 N2154 zs=cw, cw+1, ..., cw+weeks_to_fetch-1
  （受 jwc::throttle 限制；若 throttle 不允许并发 → 串行 + tracing 一次 warn）
        │
        ▼
对每个返回的 kb_list，按 oldzc 过滤本周课
        │
        ▼
对每节课展开 jc_positions(old_jc) → 多个 jc
        │
        ▼
为每个 (kc, week, xqj, jc) 算 absolute_datetime = rqazc_list[xqj-1].rq + period_clock.lookup(jc).0
        │
        ▼
filter: absolute_datetime > now()
sort by absolute_datetime ascending
take K
```

**边界**：`weeks_to_fetch > 5` 直接 reject（spec 已限 within ≤ 31）；若 cw + weeks_to_fetch - 1 > 18，envelope 附 `hint: "exceeds semester end (week 18)"` 并截断到合法周。

## 8. Output Envelope

### 8.1 `today` / `week` 通用结构

```json
{
  "ok": true,
  "data": {
    "xnm": "2025",
    "xqm": "12",
    "current_week": 14,
    "today_iso": "2026-05-12",
    "today_weekday": 2,
    "query_zs": 14,
    "rqazc_list": [
      { "xqj": 1, "rq": "2026-05-11" },
      { "xqj": 2, "rq": "2026-05-12" },
      { "xqj": 3, "rq": "2026-05-13" },
      { "xqj": 4, "rq": "2026-05-14" },
      { "xqj": 5, "rq": "2026-05-15" },
      { "xqj": 6, "rq": "2026-05-16" },
      { "xqj": 7, "rq": "2026-05-17" }
    ],
    "kb_list": [
      {
        "kcmc": "高等数学II",
        "xqj": 2,
        "jc_list": [3, 4],
        "clock_list": [["10:00","10:45"], ["10:55","11:40"]],
        "jcor_fallback": "3-4",
        "cdmc": "东中院 1-101",
        "xm": "张老师",
        "kch_id": "MA002",
        "old_zc": 524286,
        "old_jc": 12,
        "in_this_week": true
      }
    ],
    "meta": {
      "bitmask_fallback": false,
      "period_clock_source": "zf_dict"
    }
  }
}
```

### 8.2 `next` 特有字段

```json
{
  "ok": true,
  "data": {
    "current_week": 14,
    "within_days": 7,
    "limit": 5,
    "fetched_weeks": [14, 15],
    "next_list": [
      {
        "kcmc": "高等数学II",
        "datetime_start": "2026-05-12T10:00:00",
        "datetime_end": "2026-05-12T11:40:00",
        "week": 14,
        "xqj": 2,
        "jc_list": [3, 4],
        "cdmc": "东中院 1-101",
        "xm": "张老师"
      }
    ]
  }
}
```

### 8.3 hint / error envelope

```json
// 学期未开始 / 已结束
{ "ok": true, "data": { ..., "hint": "学期已结束 / 假期" } }

// N2154 字段缺失
{ "ok": false, "error": { "code": "parse_error", "message": "N2154 字段 oldzc 缺失",
    "raw_snippet": "{\"items\":[{\"kcmc\":\"...\",\"oldzc\":null}]}" } }
```

**Envelope additive 保证**：现有 N2151 `Schedule` 序列化结构不变（新增 `old_zc` / `old_jc` / `rqazc_list` 均为 `Option`，序列化时若 `None` 走 `serde(skip_serializing_if = "Option::is_none")`，N2151 的 envelope 旧消费者 0 感知）。

## 9. Error Handling

| 场景 | 行为 | 退出码 |
|---|---|---|
| N2154 端点 ZF 改版（关键字段缺失） | Envelope `error: parse_error` + `raw_snippet`，不 panic | 2 |
| `infer_current_week` cache 损坏 | 删 cache → 重调 zs=1 → 再失败 → `current_week = 1` + hint | 0 |
| `current_week` > 18 | `kb_list = []` + `hint: "学期已结束 / 假期"` | 0 |
| `--within > 31` | clap 层 reject：`--within must be 1..=31` | 2 |
| `--zs` 不在 1..=18 | clap 层 reject | 2 |
| period_clock T0 调研失败 + fallback 表也未填 | clock_list 输出 `null` + envelope.meta `period_clock_source: "missing"`，**不阻塞主流程** | 0 |
| 用户未登录 / session 过期 | 继承 `cas_login` 自动刷新 + ensure_sp_bound 已有重绑定 | 0（重绑定后）/ 1（最终失败）|
| ZF throttle 拒绝 | 继承现有 throttle.rs 的指数退避；3 次失败后 envelope.error | 1 |
| 跨学期（cw + weeks_to_fetch > 18）| 截断到 18 + envelope.hint | 0 |

## 10. Testing 策略

### 10.1 单元测试（`cargo test`）

```rust
// src/apps/jwc/period_clock.rs::tests
#[test] fn lookup_jc_3_returns_10_00_to_10_45()
#[test] fn lookup_jc_0_returns_none()
#[test] fn lookup_jc_14_returns_none()

// src/apps/jwc/api/schedule.rs::tests
#[test] fn is_in_week_old_zc_524286_returns_true_for_weeks_2_to_18() // = 0b111111111111111110
#[test] fn is_in_week_week_0_returns_false()
#[test] fn jc_positions_old_jc_12_returns_3_and_4() // = 0b1100
#[test] fn jc_positions_old_jc_0_returns_empty()

// src/commands/jwc/schedule_handlers.rs::tests
#[test] fn infer_current_week_from_2025_09_08_to_2026_05_12_returns_36()
//   (实际算 = 247 / 7 + 1 = 36，但 ZF cap 18 后应触发 hint)
#[test] fn infer_current_week_within_range_returns_correct_value()
#[test] fn cmd_today_filters_out_courses_with_old_zc_not_matching_cw()
#[test] fn cmd_next_within_1_returns_only_remaining_today()
#[test] fn cmd_next_within_31_caps_weeks_to_fetch_at_5()
```

### 10.2 集成测试（mockito 起本地 HTTP）

```rust
// tests/jwc_schedule_derivatives.rs
#[test] fn n2154_full_flow_zs_14_returns_filtered_kb_list()
#[test] fn n2154_response_missing_oldzc_falls_back_to_jcor_zcd_parse()
#[test] fn n2154_response_corrupt_returns_parse_error_envelope_not_panic()
```

**Fixture 来源**：T0 完成后由主对话用 `sjtu login` + 自定义 debug `cargo run --bin sjtu jwc schedule-by-week --zs 14`（plan 阶段会加这个临时 hidden flag）真机抓一次 N2154 zs=1 + zs=14 响应，脱敏后落 `tests/fixtures/jwc/`。

### 10.3 真机测试（`cargo test -- --ignored`）

```rust
#[tokio::test]
#[ignore]
async fn real_machine_cmd_today_works() {
    // 需先 sjtu login；CI 跳过
}
```

## 11. Cross-cutting

### 11.1 i.sjtu 红线复核

| 红线项 | 本 spec 行为 | 合规性 |
|---|---|---|
| 不动个人信息 | 无信息维护相关调用 | ✅ |
| 不动选课内容 | 无选课调用 | ✅ |
| 无 form.submit() | N2154 是 POST form 但目标 `cxXsKb` 是查询路径 | ✅（参考 §2.7 调研结论） |
| chrome-devtools 只读 | T0 调研只用 take_snapshot / evaluate_script（只读 JS） | ✅ |
| 不替用户决策状态变更 | 全 read-only | ✅ |

### 11.2 隐私

- cache 文件 `~/.cache/sjtu-cli/jwc_week_cache.json`：**仅含 `{xnm-xqm: {week, fetched_at}}`**，**不含学号 / 姓名 / 任何 PII**
- envelope 输出含 `kch_id` / `kcmc` / `xm`（教师姓名）等公开教务信息，与现有 N2151 / grades 输出口径一致
- 日志（tracing）输出始终走现有脱敏路径（cookies 只打前 8 位 + `***`）

### 11.3 跨平台路径（cache 文件落点）

调用：`ProjectDirs::from("edu", "sjtu", "sjtu-cli")`

| 平台 | session.json (`config_dir()`) | jwc_week_cache.json (`cache_dir()`) |
|---|---|---|
| Linux | `~/.config/sjtu-cli/session.json` | `~/.cache/sjtu-cli/jwc_week_cache.json` |
| macOS | `~/Library/Application Support/edu.sjtu.sjtu-cli/session.json` | `~/Library/Caches/edu.sjtu.sjtu-cli/jwc_week_cache.json` |
| Windows | `%APPDATA%\sjtu\sjtu-cli\config\session.json` | `%LOCALAPPDATA%\sjtu\sjtu-cli\cache\jwc_week_cache.json` |

注：macOS / Windows 上 `directories` crate 会以 `<qualifier>.<organization>.<application>` 反向域名风格组织 `<org>\<app>` 子层；Linux 仅用 application 名作单级目录（XDG 习惯）。已联网验证 directories crate latest 文档。

**为什么 cache 不能塞进 config_dir**：
- XDG / OS 标准明确 cache 是"可重建临时数据"、config 是"用户凭证 / 偏好"
- 系统级 cache 清理工具（Linux bleachbit / macOS App Cleaner / Windows Disk Cleanup）可一键清 cache 不动 session
- macOS Time Machine 默认跳过 Caches；Windows Local（非 Roaming）不跨机同步——cache 本就不该被备份/同步
- `directories` crate 已在依赖里，0 新依赖

### 11.4 Envelope additive 保证

| 字段 | 已有结构 | 加新字段 | 旧消费者兼容 |
|---|---|---|---|
| `KbItem` | 现有 N2151 fields | `+old_zc: Option<u32>` `+old_jc: Option<u32>` | ✅（`serde(skip_serializing_if = "Option::is_none")`） |
| `Schedule` | 现有 `xqjmc_map` / `kb_list` | `+rqazc_list: Vec<RqAzc>`（N2151 路径恒为空 Vec → serde 序列化为 `[]`） | ✅ |
| 新 `TodayData` / `WeekData` / `NextData` | 全新 struct | - | ✅（不和现有 Envelope payload 冲突） |

### 11.5 单文件 200 行硬限

详见 §6.3。3 处拆分均在本 spec 内显式列出，plan 阶段不会临时决策。

## 12. Pre-tasks / Out of Scope

### 12.1 Pre-tasks（plan 阶段第 0 步）

**T0 — ZF 节次字典端点调研**（主对话亲跑，subagent 无 chrome 句柄）

- 工具：mcp__chrome-devtools（只读 JS / take_snapshot）
- 目标：找到 ZF 9 字典端点（候选 `/xtgl/zdpz_cxZdpzList.html?gnmkdm=N2151`），输出节次 → 时刻表
- 输出：
  - 若找到 → `period_clock.rs::fetch_from_zf_dict()` helper + 常量表填实际值
  - 若失败 → 节次表用 SJTU 教务处公开页面值人工录入，附信源 URL in code 注释
- 时间预算：≤ 1h

### 12.2 Out of Scope（推迟 v0.3+）

| # | 项 | 推迟原因 |
|---|---|---|
| OoS1 | iCal / ics 导出 | T5 单独 spec，与本 T1 解耦 |
| OoS2 | 课程冲突检测 / 时间冲突告警 | 衍生需求，等 today/week/next 实装后再评估 |
| OoS3 | 跨学期 schedule（一学年视图）| N2151 已覆盖单学期全量；跨学期需 1 学年视图，留 T2.5 |
| OoS4 | `--week-template` / 课表换肤 / 颜色 | UI 层，等用户反馈再做 |
| OoS5 | 课表持久化缓存（不只反推周次）| 性能优化，先看真机延迟是否需要 |
| OoS6 | mock server 自动 fixture 录制 | DX 工具，CP-V 系列已有先例，单独 spec |

## 13. 决策记录（brainstorming 阶段已敲定）

| # | 问题 | 选择 | 理由 |
|---|---|---|---|
| D1 | 节次→时刻映射来源 | T0 chrome-devtools 调研 ZF 字典端点 + fallback 硬编码 | 准确性优先；调研失败有退路 |
| D2 | 命令路径 | `sjtu jwc today / week / next` | 与现有 `sjtu jwc grades/schedule/gpa/exams` 平级，发现性最好 |
| D3 | TTY 输出 | 双模式：默认列表 + `--grid` flag | 列表对 AI Agent 友好（YAML/JSON），grid 对人友好 |
| D4 | `next` 语义 | `--within N`（1..=31）+ `--limit K` | 整月覆盖；N>7 时并发拉 ≤ 5 个 N2154 |
| D5 | 当前周次反推 | N2154 zs=1 → rqazc_list[0].rq 反推 | 零额外端点；24h cache 后续调用快 |
| D6 | 字段命名 | ZF 拼音保留（kcmc / xqj / jcor / zcd / oldzc / oldjc） | 与 `isjtu_investigation.md` + 现有 grades/gpa/exams 一致 |
| D7 | cache 落点 | `directories::cache_dir()`（不与 session 同目录） | XDG / OS 标准；联网验证最优 |

## 14. Open Questions（plan 阶段决定）

- **OQ1**：throttle.rs 是否允许并发 N2154 调用？若不允许 → next within=31 串行 5 次 → 真机延迟需测；若允许 → 并发。**plan 阶段读 `src/apps/jwc/throttle.rs` 后决定**。
- **OQ2**：`xnm` / `xqm` 为空时 cache key 用 `"current"` + TTL 1h，是否足够？还是首次调用强制要先 echo 出实际 xnm/xqm？**plan 阶段先按 MVP（"current" + 1h）实装，真机测后再决**。
- **OQ3**：`--grid` 在窄终端（< 80 列）的退化策略？**plan 阶段先实装"宽度 ≥ 80 才 grid，否则 warn + 走列表"**。

---

**Spec 自审 checkpoint**（写完此 spec 后做）：

- [ ] Placeholder scan：无 TBD / TODO（period_clock 的 fallback 表是 placeholder，由 plan 的 T0 任务填）
- [ ] 内部一致性：D5 + §7.1 + §7.4 算法对齐
- [ ] Scope：3 命令 + 1 endpoint + 1 cache + T0 调研，可在 1 plan 内完成
- [ ] Ambiguity：跨学期 within 行为已显式（§9 "截断到 18 + hint"）

---

**Supersedes**：无（首次 spec）
**Followed by**：`docs/superpowers/plans/2026-05-12-jwc-schedule-derivatives.md`（实装计划，brainstorming 通过后写）
