# T2 jwc GPA 计算 + 学期均分 + 班级 / 专业排名 — Design Spec

> **状态**：Draft → 待用户过审 → 转 writing-plans
> **日期**：2026-05-12
> **范围**：补完 jwc 子系统的 GPA / 学积分 / 排名查询（单学期 + 多学期对比），不动现有 S3f 已落地的 grades 单课成绩链路
> **复用基线**：N309131 ZF SP 两阶段（已实装 + 单测）/ jwc CAS（5878fba staleness-fix 已通）/ JwcPage envelope / Envelope render

---

## 1. 背景与现状

### 1.1 现状基线（T1 副产品已实装）

T1 schedule-derivatives 在 2026-05-12 顺带完成了 GPA 子模块的核心实装，目前**未做收尾**：

| 层 | 文件 | 状态 |
|----|------|------|
| Model | `src/apps/jwc/models/gpa.rs` (60 行) | ✅ `Gpa` struct 14 字段 |
| API | `src/apps/jwc/api/gpa.rs` (135 行) | ✅ N309131 两阶段 `client.gpa(scope, rank, qs, zz)` |
| CmdData | `src/commands/jwc/data.rs::GpaData` | ✅ envelope shape |
| Handler | `src/commands/jwc/handlers.rs::cmd_gpa` | ✅ render → Envelope |
| CLI | `src/cli/jwc/mod.rs::JwcSub::Gpa` | ✅ `sjtu jwc gpa --scope --rank --from --to` |
| 单测 | `src/apps/jwc/tests_parse.rs` | ✅ step1 裸字符串 + step2 envelope 两 case |

### 1.2 待补的 4 个 gap

1. **排名 server-side 仅给字符串 `"X/Y"`**：agent 拿到要再 split 一遍，不友好；用户要求**双轨**（保留原字符串 + 附加 parsed 结构化字段）
2. **多学期对比缺失**：N309131 一次只能查单学期窗口；用户要求**新增 `sjtu jwc gpa-by-semester`** 客户端循环聚合
3. **fixture 缺失**：tests/fixtures/jwc/ 没有 N309131 真实响应；集成测试只有 inline string
4. **真机 smoke 未跑**：6 组合（hxkc/qbkc × njzy/nj/bj）+ multi-semester 未实地验

### 1.3 关键技术约束（继承）

- **i.sjtu 硬红线**：只读；不提交任何表单 / 不点任何"提交/保存/绑定"按钮
- **金额 / 排名**：禁用 f32/f64 表示精度敏感量；GPA / 学积分本身是 server 给的字符串，原样保留
- **行数硬限**：单源文件 200 行 / 单测文件 300 行 / 单模块 2000 行
- **不引入新依赖**：复用 reqwest / tokio / serde / chrono
- **Envelope additive**：所有新字段只加不改，旧字段语义不动

---

## 2. Goals / Non-Goals

### Goals

- G1 — 单学期 GPA 查询的排名字段**双轨输出**（原字符串 + parsed RankPair）
- G2 — 新增 `sjtu jwc gpa-by-semester` 自动循环 N×3 学期、聚合输出多学期对比数据
- G3 — 真机 fixture 落盘 + mockito 集成测试两阶段串联
- G4 — 真机 smoke 矩阵覆盖（6 单学期组合 + 1 多学期）
- G5 — 文档收尾（README / SKILL / tasks/todo.md / tasks/lessons.md）

### Non-Goals

- **NG1**：不重算 GPA。server 给什么用什么；不在 client 端做 4 分制 / 5 分制 / 5.3 算法切换（教务处官方算法是黑盒，自算只会与 server 偏差）
- **NG2**：不做单课成绩 → GPA 的 client 聚合（N305005 没有 jd 加权所需的精确学分占比，会失真）
- **NG3**：不做"排名历史趋势图"（终端 ASCII 图非本期范围，留 T3）
- **NG4**：不动 `cmd_grades`（S3f 已通，N305005 无排名字段，不强行嫁接）
- **NG5**：不实装 `--by-semester` flag 形式（用独立子命令 `gpa-by-semester`，clap 派发清晰、help 输出干净）

---

## 3. 架构总览

```
┌───────────────────────────────────────────────────────┐
│  CLI 层 (src/cli/jwc/mod.rs)                          │
│                                                       │
│  JwcSub::Gpa { scope, rank, from, to }                │  (现状)
│  JwcSub::GpaBySemester { scope, rank, xnm_from,       │  (新)
│                          xnm_to }                     │
└─────────┬───────────────────────────┬─────────────────┘
          │                           │
          ▼                           ▼
┌─────────────────────┐   ┌─────────────────────────────┐
│ commands/jwc/       │   │ commands/jwc/               │
│   handlers.rs       │   │   gpa_handlers.rs (新)      │
│                     │   │                             │
│ cmd_gpa()           │   │ cmd_gpa_by_semester()       │
│ ├─ client.gpa()×1   │   │ ├─ enumerate_semesters()    │
│ └─ fill_parsed()    │   │ ├─ loop client.gpa()        │
│   (新增)            │   │ │   + 600ms throttle        │
└─────────┬───────────┘   │ ├─ fail-soft 双数组         │
          │               │ └─ fill_parsed()            │
          │               └────────┬────────────────────┘
          │                        │
          └────────┬───────────────┘
                   ▼
        ┌──────────────────────────────┐
        │ apps/jwc/api/gpa.rs (现状)   │
        │ client.gpa(scope, rank,      │
        │            qs_xnxq, zz_xnxq) │
        │ → N309131 step1 + step2      │
        └──────────────────────────────┘
                   ▼
        ┌──────────────────────────────┐
        │ apps/jwc/models/gpa.rs       │
        │ Gpa { 14 字段 + 2 parsed (新)│
        │ RankPair { rank, total,      │
        │            percentile } (新) │
        │ parse_rank() (新)            │
        └──────────────────────────────┘
```

**核心改动局部化**：API 层完全不动；新增能力集中在 model（parse_rank + RankPair）+ 新 handler 文件（gpa_handlers.rs）+ CLI 新 variant。

---

## 4. 数据形状

### 4.1 `RankPair`（新增到 models/gpa.rs）

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RankPair {
    pub rank: u32,
    pub total: u32,
    /// 百分位 = rank / total × 100。total=0 时为 None。
    /// rank > total 时也设 None（数据异常但不报错）。
    pub percentile: Option<f64>,
}

/// fail-soft 解析 ZF 排名字符串 `"X/Y"`。
///
/// 容错策略：
/// - 空字符串 / 无斜杠 / 非数字 → None
/// - total=0 → Some(RankPair { rank, total:0, percentile:None })
/// - rank>total → Some(RankPair { rank, total, percentile:None })
pub fn parse_rank(s: &str) -> Option<RankPair> {
    let trimmed = s.trim();
    if trimmed.is_empty() { return None; }
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

### 4.2 `Gpa` struct 扩展（apps/jwc/models/gpa.rs）

在原 14 字段尾部追加两条 client-only 字段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Gpa {
    // 原 14 字段全部保持不动（gpa / gpapm / xjf / xjfpm / zf / ms /
    //                           bjgmc / bjgms / zxf / hdxf / bjgxf / tgl / kcfw / czsj）
    // ... 略 ...

    /// 客户端从 gpapm 解析的结构化排名。server 不返回此字段。
    #[serde(default, skip_deserializing)]
    pub gpapm_parsed: Option<RankPair>,

    /// 客户端从 xjfpm 解析的结构化排名。server 不返回此字段。
    #[serde(default, skip_deserializing)]
    pub xjfpm_parsed: Option<RankPair>,
}

impl Gpa {
    /// 从 gpapm / xjfpm 字符串解析填充 parsed 字段。fail-soft：
    /// 解析失败时对应字段保持 None，不抛错。供 cmd_gpa /
    /// cmd_gpa_by_semester 收到 envelope 后统一调用。
    pub fn fill_parsed(&mut self) {
        self.gpapm_parsed = self.gpapm.as_deref().and_then(parse_rank);
        self.xjfpm_parsed = self.xjfpm.as_deref().and_then(parse_rank);
    }
}
```

**关键 serde 决策**：
- `skip_deserializing`：server 响应里不会有这字段，反序列化跳过避免 schema 漂移误报
- 不加 `skip_serializing`：序列化照常输出（YAML/JSON agent 拿全）
- `default`：parsed 为 None 时序列化为 `null`（YAML 友好）

### 4.3 `GpaBySemesterData`（新增到 commands/jwc/data.rs）

```rust
#[derive(Debug, Serialize)]
pub(super) struct GpaBySemesterData {
    pub scope: &'static str,
    pub rank: &'static str,
    pub xnm_from: u32,
    pub xnm_to: u32,
    /// 请求过的全部 (xnm, xqm) 组合，含已落空的。
    pub requested: Vec<SemesterKey>,
    /// 成功拿到 GPA 的学期，含 parsed。
    pub succeeded: Vec<SemesterGpa>,
    /// 失败学期：(xnm, xqm, error_msg)。
    /// 三种 case 均归入：server 拒绝 / items 空 / 网络挂。
    pub failed: Vec<SemesterFailure>,
}

#[derive(Debug, Serialize)]
pub(super) struct SemesterKey {
    pub xnm: String,
    pub xqm: String,
}

#[derive(Debug, Serialize)]
pub(super) struct SemesterGpa {
    pub xnm: String,
    pub xqm: String,
    pub gpa: Option<String>,
    pub gpapm: Option<String>,
    pub gpapm_parsed: Option<RankPair>,
    pub xjf: Option<String>,
    pub xjfpm: Option<String>,
    pub xjfpm_parsed: Option<RankPair>,
    pub ms: Option<String>,           // 满分（标记位）
    pub zxf: Option<String>,          // 总学分
    pub czsj: Option<String>,          // server 操作时间戳
}

#[derive(Debug, Serialize)]
pub(super) struct SemesterFailure {
    pub xnm: String,
    pub xqm: String,
    pub reason: String,                // "未到统计时间" / "网络失败" / "items 空" / 原始 err
}
```

---

## 5. 客户端循环（gpa-by-semester）

### 5.1 学期枚举

固定枚举 xqm `["3", "12", "16"]`（秋 / 春 / 夏季小学期）。xnm 区间 `xnm_from..=xnm_to`。

**默认值**：
- `--xnm-from` 默认 = 当年 - 3（覆盖 4 年制本科典型场景）
- `--xnm-to` 默认 = 当年
- 当年来自 `chrono::Local::now().year() as u32`，不依赖 sub_session

**示例**：2026-05 跑默认 → xnm ∈ {2023, 2024, 2025, 2026}, xqm ∈ {3, 12, 16} → 12 个 (xnm, xqm) 组合。

**OQ**：非 4 年学制（研究生 / 留学生）需手动给 `--xnm-from`；文档里写明，不在 client 自动嗅探。

### 5.2 循环 + throttle

```rust
const SEMESTER_QUERY_THROTTLE_MS: u64 = 600;
const XQM_LIST: &[&str] = &["3", "12", "16"];

for xnm in xnm_from..=xnm_to {
    for xqm in XQM_LIST {
        let xnxq = format!("{xnm}{xqm}");
        let res = client.gpa(scope, rank, Some(&xnxq), Some(&xnxq)).await;
        tokio::time::sleep(Duration::from_millis(SEMESTER_QUERY_THROTTLE_MS)).await;
        match res {
            Ok(env) if !env.items.is_empty() => {
                let mut g = env.items.into_iter().next().unwrap();
                fill_parsed(&mut g);
                succeeded.push(g.into());  // Gpa -> SemesterGpa 字段截选
            }
            Ok(_) => failed.push(SemesterFailure {
                xnm: xnm.to_string(), xqm: xqm.to_string(),
                reason: "items 空（疑似未到统计时间或该学期无成绩）".into(),
            }),
            Err(e) => failed.push(SemesterFailure {
                xnm: xnm.to_string(), xqm: xqm.to_string(),
                reason: e.to_string(),
            }),
        }
    }
}
```

**关键设计**：
- **throttle 600ms**：对齐 T1 schedule 8 周循环（已实地通过）。12 学期 × 0.6s ≈ 7.2s 总耗时
- **fail-soft 不抛错**：单学期失败进 `failed` 数组，整体 exit code 仍为 0
- **解析独立步骤**：fill_parsed 内嵌在 succeeded push 之前，handlers 与 gpa_handlers 共用

### 5.3 fill_parsed 调用点

填充逻辑作为 `impl Gpa` 方法定义在 §4.2 models/gpa.rs，commands 层调用形式：

```rust
// cmd_gpa（handlers.rs）：
let mut env_resp = client.gpa(scope, rank, qs, zz).await?;
for g in &mut env_resp.items { g.fill_parsed(); }

// cmd_gpa_by_semester（gpa_handlers.rs）：
let mut g = env.items.into_iter().next().unwrap();
g.fill_parsed();
succeeded.push(g.into());
```

放在 models 层的原因：避免 commands 子模块跨文件 `pub(super)` helper 调用（mod 边界复杂），且 Gpa 自描述更优雅。

---

## 6. CLI 形态

### 6.1 现状 `sjtu jwc gpa`（仅追加 parsed 填充，签名不变）

```
sjtu jwc gpa [--scope hxkc|qbkc] [--rank njzy|nj|bj]
             [--from <YYYYxqm>] [--to <YYYYxqm>]
             [-o yaml|json|table]
```
默认 `--scope hxkc --rank njzy`，from/to 不给则查全部时段累计。

### 6.2 新增 `sjtu jwc gpa-by-semester`

```
sjtu jwc gpa-by-semester [--scope hxkc|qbkc] [--rank njzy|nj|bj]
                          [--xnm-from <YYYY>] [--xnm-to <YYYY>]
                          [-o yaml|json|table]
```

**clap variant**：
```rust
/// 多学期 GPA 对比：客户端循环 4 年 × 3 学期 N309131，聚合输出
GpaBySemester {
    #[arg(long, value_enum, default_value_t = GpaScopeArg::Hxkc)]
    scope: GpaScopeArg,
    #[arg(long, value_enum, default_value_t = GpaRankArg::Njzy)]
    rank: GpaRankArg,
    /// 起始学年（默认当年-3，覆盖 4 年制本科）
    #[arg(long)]
    xnm_from: Option<u32>,
    /// 结束学年（默认当年）
    #[arg(long)]
    xnm_to: Option<u32>,
    #[arg(short = 'o', long, value_enum)]
    output: Option<OutputFormat>,
},
```

**dispatch**：
```rust
JwcSub::GpaBySemester { scope, rank, xnm_from, xnm_to, output } => {
    jwc_cmds::cmd_gpa_by_semester(
        scope.into(), rank.into(), xnm_from, xnm_to, output
    ).await
}
```

---

## 7. 测试策略

### 7.1 单元测试（src/apps/jwc/tests_parse.rs 续写）

- `parse_rank_normal` — `"3/120"` → `Some({rank:3, total:120, percentile:Some(2.5)})`
- `parse_rank_total_zero` — `"0/0"` → `Some({rank:0, total:0, percentile:None})`
- `parse_rank_empty` — `""` → None
- `parse_rank_no_slash` — `"abc"` / `"3"` → None
- `parse_rank_overflow` — `"200/120"` → `Some({rank:200, total:120, percentile:None})`
- `parse_rank_with_whitespace` — `"  3 / 120  "` → 与 normal case 同（fail-soft trim）

### 7.2 集成测试（mockito + fixture）

新建集成测试块：复用 `tests_parse.rs` 或新文件 `tests_gpa_integration.rs`（< 300 行），优先扩 `tests_parse.rs`，超过 300 行再拆。

- `gpa_step1_step2_round_trip` — 启 mockito 拼 step1（200 + 裸字符串 `"统计成功！"`）→ step2（200 + fixture JSON），client.gpa() 端到端拿 envelope，验关键字段
- `gpa_step1_failure_aborts` — step1 返非 "统计成功！"（如 `"未到统计时间"`），client.gpa() 应抛 Err，step2 不被请求
- `gpa_step2_empty_items` — step1 ok 但 step2 envelope `items: []`，client.gpa() 返回 env.items 空，由上层 handler 决定是否归 failed

### 7.3 真机 fixture（T0 主对话亲跑）

**抓取**：
- 主对话用浏览器 / curl 经 jwc CAS 拿 sub_session，跑 N309131 step1+step2
- 仅 1 组合：`scope=hxkc, rank=njzy, qs=zz=<当本学期>`

**脱敏**：删除以下字段（用 jq / Python）：
- `xh` / `xm` / `xh_id` / `bj` / `jgmc` / `zymc` / `njmc` / `bjgmc` / `bjgms`

**保留**（供测试断言）：
- `gpa` / `gpapm` / `xjf` / `xjfpm` / `zf` / `ms` / `zxf` / `hdxf` / `bjgxf` / `tgl` / `kcfw` / `czsj`

**落盘**：
- `tests/fixtures/jwc/n309131_step1_success.txt` — 内容 `"统计成功！"`（含双引号，即原裸 JSON 字符串）
- `tests/fixtures/jwc/n309131_step2_hxkc_njzy.json` — 脱敏 envelope（保留 currentPage/pageSize/totalResult/items 结构）

### 7.4 真机 smoke 矩阵（T5 主对话亲跑）

| 组合 | 命令 | 预期断言 |
|------|------|----------|
| 1 | `sjtu jwc gpa --scope hxkc --rank njzy` | items[0].gpapmParsed.percentile 数值有效 |
| 2 | `sjtu jwc gpa --scope hxkc --rank nj` | 同上，但 total 数值显著更大 |
| 3 | `sjtu jwc gpa --scope hxkc --rank bj` | 同上，但 total ≈ 班级人数（30-60） |
| 4 | `sjtu jwc gpa --scope qbkc --rank njzy` | gpa 数值可能与 hxkc 不同 |
| 5 | `sjtu jwc gpa --scope qbkc --rank nj` | 同上 |
| 6 | `sjtu jwc gpa --scope qbkc --rank bj` | 同上 |
| 7 | `sjtu jwc gpa-by-semester` | succeeded.len() ≥ 1，failed 数组按学期标注 reason |

---

## 8. 风险 / Open Questions

### OQ1：N309131 step1 对"未到统计时间"学期是否一定返非"统计成功！"

- **影响**：若 step1 总返成功而 step2 给 items 空，client 端只能用空 items 判定 → 当前已支持（`Ok(env) if !env.items.is_empty()` 分支处理）
- **解决时机**：T0 调研时实地试 1-2 个未来学期或空选课学期
- **影响范围**：若 step1 真返非成功字符串，需在 fail-soft 文案里覆盖一种特定 reason 串（"未到统计时间"）

### OQ2：12 学期循环是否触发 i.sjtu 风控

- **影响**：12 × 0.6s ≈ 7.2s，理论上低于 schedule 8 周（实测安全）
- **解决时机**：T5 真机 smoke 实地验
- **mitigation**：若被风控，throttle 加到 1200ms（仍 < 15s 整体）；不引入并发

### OQ3：`--xnm-from` 默认 "当年-3" 对非 4 年学制失真

- **影响**：研究生 2-3 年制 / 博士 4-5 年制 / 留学生预科会查到空学期（多余的 failed 条目，不影响 succeeded）
- **解决方案**：接受。文档写明手动给 `--xnm-from` 即可；不做 client 嗅探（嗅探需要查毕业信息表，超 NG3 范围）

### OQ4：`Gpa` -> `SemesterGpa` 字段截选时丢字段

- **影响**：SemesterGpa 比 Gpa 少 5 字段（hdxf/bjgxf/tgl/kcfw/bjgmc/bjgms/bjgxf 等）。多学期对比场景下用户更关心 GPA/排名/学分，明细字段不重要
- **mitigation**：若用户实地反馈缺字段，T6 文档后续 patch 加回；当前先按精简版交付

---

## 9. 文件结构与行数预算

### 9.1 新建

| 路径 | 用途 | 预估行数 |
|------|------|----------|
| `src/commands/jwc/gpa_handlers.rs` | `cmd_gpa_by_semester` + `enumerate_semesters` + `fill_parsed` | ~130 |
| `tests/fixtures/jwc/n309131_step1_success.txt` | T0 真机脱敏 | 1 |
| `tests/fixtures/jwc/n309131_step2_hxkc_njzy.json` | T0 真机脱敏 | ~40 |

### 9.2 修改

| 路径 | 现状 | 变更 | 预估终态 |
|------|------|------|----------|
| `src/apps/jwc/models/gpa.rs` | 60 行 | +RankPair +parse_rank +2 字段 +impl Gpa::fill_parsed | ~135 |
| `src/apps/jwc/tests_parse.rs` | 157 行 | +6 parse_rank case + 3 集成 case | ~250 |
| `src/commands/jwc/data.rs` | 200 行 | +GpaBySemesterData +SemesterKey +SemesterGpa +SemesterFailure | **需拆**：已撞 200 行硬限 → 拆出 `data/gpa.rs`（详见 §9.3） |
| `src/commands/jwc/handlers.rs` | 125 行 | +`for g in &mut items { g.fill_parsed() }` 调用（cmd_gpa 内） | ~130 |
| `src/commands/jwc/mod.rs` | 17 行 | +`pub use gpa_handlers::cmd_gpa_by_semester` | ~20 |
| `src/cli/jwc/mod.rs` | 165 行 | +GpaBySemester variant + dispatch arm | ~190 |

### 9.3 200 行硬限风险点

- **commands/jwc/data.rs 已在 200 行边缘**：新增 4 个 struct 必然破线 → **plan 阶段决定拆分方案**（候选：拆出 `commands/jwc/data/mod.rs` + `data/gpa.rs`，把 GpaData / GpaBySemesterData / SemesterGpa / SemesterFailure 全部移出）
- **cli/jwc/mod.rs 165 → 190**：仍在 200 内，安全
- **handlers.rs 126 → 135**：仍在 200 内，安全
- **新 gpa_handlers.rs ~130**：在 200 内，安全
- **tests_parse.rs 158 → 250**：仍在 300 行测试限内，安全

---

## 10. 实施顺序（高层；写 plan 时细化为 bite-sized tasks）

T0. 主对话亲跑 N309131 真机 + 脱敏 fixture
T1. RankPair + parse_rank + 6 单测
T2. Gpa struct 扩展 + fill_parsed + cmd_gpa 集成
T3. data.rs 拆分（若 plan 阶段确认必拆）
T4. gpa_handlers.rs 新建 + cmd_gpa_by_semester 实装
T5. CLI variant + dispatch
T6. 集成测试（mockito + fixture）
T7. 主对话亲跑 6 + 1 真机 smoke
T8. 文档收尾（README / SKILL / tasks/todo.md / tasks/lessons.md）

每 task 走 TDD：failing test → 实装 → cargo test + fmt + clippy → commit。预估 8-10 个 task。

---

## 11. 不变量声明（agent 集成约束）

- 旧 `Gpa` 字段语义 / 顺序 / 类型不变
- 旧 `cmd_gpa` 命令行签名不变（仅 envelope 输出新增 2 个 parsed 字段）
- 旧 `GpaData` envelope shape 不变（仅内嵌 Gpa items 多 2 个字段）
- `client.gpa()` API 签名不变（commands 层 wrap 调用 + fill_parsed）
- jwc CAS / load_sub_session / cache_is_fresh staleness 检查不动
- i.sjtu 硬红线全程遵守：所有调用走 GET，零 POST 表单
