# T8 · 文档收尾

> **前置**：T7 已 commit。
> **目标**：把 `sjtu jwc calendar` 写进所有用户面 / AI agent 面文档。
> **预计耗时**：1 小时。
> **零代码 / 零测试**，纯文档。**不引入新依赖**。

---

## 涉及文件清单（5 处）

1. `README.md` — 加 `sjtu jwc calendar` 用户面段
2. `SKILL.md` — 加 calendar envelope JSON schema（给 AI agent 调用用）
3. `tasks/todo.md` — 标记 T5 完成
4. `tasks/lessons.md` — 加 T5 教训段（含已发生的事故）
5. `CLAUDE.md` — 更新"当前阶段"行

---

## 1. README.md

### 1.1 先看现状

```powershell
Get-Content README.md | Select-String -Pattern "sjtu jwc" -Context 2,2
```

应已含 `grades` / `schedule` / `gpa` / `gpa-by-semester` / `exams` / `today` / `week` / `next` 8 个子命令。

### 1.2 加 `calendar` 段（在 `next` 段之后）

模板（基于既有 jwc 命令样式）：

```markdown
### `sjtu jwc calendar` — 校历 iCal 导出

把本学期个人课表 + 考试 + 学年校历整合成 RFC 5545 .ics 文件，导入 Google Calendar / Apple Calendar / Outlook / 手机本地日历。

```bash
# 默认导出本学期，写到 stdout
sjtu jwc calendar > schedule.ics

# 指定学年学期 + 输出到文件
sjtu jwc calendar --xnm 2025 --xqm 12 --to ~/Desktop/sjtu.ics

# 只要课表，跳过考试和校历整天事件
sjtu jwc calendar --no-academic --no-exams --to courses.ics

# JSON envelope 模式（给 AI agent 用）
sjtu jwc calendar --to /tmp/cal.ics -o json
```

**字段**：
- `--xnm` 学年 4 位，留空 = 按今天自动推断
- `--xqm` 学期：`3` 秋 / `12` 春 / `16` 夏，留空 = 按今天自动推断
- `--to` 输出文件路径，**不传**则 .ics 内容写 stdout（管道友好）
- `--no-academic` 跳过学年校历那路（不含节假日整天事件）
- `--no-exams` 跳过考试那路（不含 N358105 考试事件）

**幂等 UID**：每个事件的 UID 使用 FNV-1a 64-bit hash 基于 `<学年>_<学期>_<类型>_<课号>_<...>` 生成，重复 import 同一份 .ics **不会**产出重复事件（日历客户端会按 UID dedup）。
```

### 1.3 验证

```powershell
Get-Content README.md | Measure-Object -Line
# 看是否仍 < 一个合理上限（仓库没硬规定但建议 < 500 行）
```

---

## 2. SKILL.md

### 2.1 看现状

```powershell
Get-Content SKILL.md | Select-String -Pattern "jwc" -Context 1,1
```

### 2.2 加 calendar envelope schema 段

```markdown
### `sjtu jwc calendar --to <file> -o json` envelope schema

```json
{
  "ok": true,
  "command": "jwc.calendar",
  "data": {
    "xnm": "2025",
    "xqm": "12",
    "event_count": 87,
    "by_kind": {
      "class": 64,
      "exam": 9,
      "academic": 14
    },
    "hash_hex": "85944171f73967e8...",
    "bytes": 24576,
    "warnings": []
  },
  "error": null,
  "elapsed_ms": 1820
}
```

- `event_count`：写入 .ics 的 VEVENT 总数
- `by_kind`：按事件类型分组（class = 课表，exam = 考试，academic = 校历整天事件）
- `hash_hex`：.ics 内容 FNV-1a 64-bit hash hex，可用于幂等检测
- `bytes`：写入 .ics 的字节数
- `warnings`：fail-soft 警告（例如 "exams API 失败，已跳过 exam 事件" / "academic_calendar fixture 未找到，已跳过校历事件"）—— **空数组**表示所有 3 路均成功

**注意**：不指定 `--to` 时直接把 .ics 内容写 stdout（管道友好），**不**走 envelope。AI agent 要 envelope 必须带 `--to <path>` 显式写文件。
```

---

## 3. tasks/todo.md

### 3.1 找到 T5 段

```powershell
Select-String -Path tasks/todo.md -Pattern "T5"
```

### 3.2 加 T7-T9 完成标记

在 T5 章节末加：

```markdown
- [x] T5 Plan T6: cmd_calendar handler + envelope + fail-soft（commit 980859f）
- [x] T5 Plan T7: CLI Calendar variant + dispatch（commit <T7 SHA>）
- [x] T5 Plan T8: 文档收尾（commit <T8 SHA>）
- [x] T5 Plan T9: 用户亲跑 4 端日历 smoke（用户报告：日期 + 结果概要）
- [x] T5 整体完成：jwc 校历 iCal 导出 MVP
```

---

## 4. tasks/lessons.md

### 4.1 加 T5 教训段

```markdown
## T5 校历 iCal 导出（2026-05-XX）

### 已发生的事故 + 修复

1. **T1 `#[expect(dead_code)]` 误判 pub 字段不触发 dead_code**
   - 现象：subagent 报告 "pub 不触发 dead_code"，主对话 verify 时撤掉 allow 后发现 dead_code 实际会触发
   - 根因：`#[expect(...)]` 在 binary crate + pub + Default + serde 构造的 struct 上不命中"never constructed" lint
   - 修复：回退到 `#[allow(dead_code)]` + TODO 注释，**等下游 task（T6）真消费这些字段时再清掉**
   - 教训：lint expect 机制比 allow 更激进，对 serde-only 字段不可靠

2. **T2 multibyte fold 测试 pad 长度选错**
   - 现象：pad=70 让折行落在 ASCII 重复处，没测到 CJK 边界
   - 修复：pad=65 累计 73 字节 + 3 字节 CJK 字符触发折行，断言 `\r\n 操` 出现
   - 教训：RFC 5545 75-octet 折行 UTF-8 安全测试要**对准 octet 边界 + UTF-8 多字节起点**

3. **T4 events.rs 213 行 > 200 限**
   - 现象：subagent 报告 199 行，主对话 `wc -l` 实测 213 行
   - 根因：subagent 数行用 LF，工作区是 CRLF
   - 修复：拆 fnv1a_64 / make_uid 到新文件 uid.rs（19 行），events.rs 降到 194 行
   - 教训：**主对话必须亲跑 wc / measure 验证行数**，不信 subagent 口述

4. **T6 plan API path 错 10 处**
   - 现象：plan 中用 `crate::apps::jwc::api::*`（private mod），实际 client.exams 是 4-arg 返回 JwcPage<Exam>
   - 修复：主对话 brief 实装 subagent 时**明确列 10 处修正**，让 subagent 用 re-export + `.items` 转 Vec
   - 教训：plan 写到 ~80% 时不要再追求"100% 准确" —— 把"主对话 brief 时补差"作为合理 buffer

### 设计决策

- **FNV-1a 64-bit 手卷 hash** 代替 sha1 依赖：UID 不要求密码学强度，FNV-1a 16 字符 hex 足够保 dedup 唯一
- **fail-soft 三路并发**：`tokio::join!` 让 schedule / exams / academic 三路独立失败；任一失败只往 `warnings` 加一条，不阻塞其余两路
- **--to + stdout 双模**：不传 `--to` 直接 .ics 写 stdout（管道友好），传了就 envelope JSON 模式（AI agent 友好）
```

---

## 5. CLAUDE.md

### 5.1 找到 "当前阶段" 段

```powershell
Select-String -Path CLAUDE.md -Pattern "当前阶段"
```

### 5.2 改"已完成"行 + "下一步"行

在 `已完成` 行末追加：` / S3f-T5 jwc 校历 iCal MVP — 课表+考试+校历 三路 fail-soft + RFC 5545 + FNV-1a UID 幂等`

把 `下一步` 行改为：`S3 Phase 2 候选 — 一卡通明细 / 通知聚合 / 图书馆借阅；或继续 jwc（培养方案 / 选课结果只读查询）`

---

## 6. 验证 + commit

```powershell
# fmt / clippy 应不受文档影响，但保险跑一遍
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings 2>&1 | Select-Object -Last 5

# 看改了哪些
git diff --stat

# commit
git add README.md SKILL.md tasks/todo.md tasks/lessons.md CLAUDE.md
git commit -m @'
docs(jwc): T5 校历 iCal 导出文档收尾（T5 Task 8）

- README.md 加 sjtu jwc calendar 用户面用法 + 5 个 flag 说明
- SKILL.md 加 calendar --to envelope JSON schema 给 AI agent
- tasks/todo.md 标 T5 Plan T7/T8 完成
- tasks/lessons.md 补 T5 4 起事故复盘 + 设计决策
- CLAUDE.md 更新 "当前阶段" 已完成/下一步

T8 后只剩 T9 用户亲跑 4 端真机 smoke，T5 整体收尾在望。

Co-Authored-By: codex
'@
```

---

## 7. 完工报告

```
STATUS: DONE
T8: 文档收尾
commit: <SHA>
改动文件：5 个（README / SKILL / todo / lessons / CLAUDE）
git diff --stat: <粘贴输出>
偏离: 无
下一步: 给用户 T9 真机 smoke checklist
```

**注意**：T8 完成后不要直接跑 T9 —— T9 是**用户亲跑**，codex 只负责给 checklist。见 `T9-user-real-machine-smoke.md`。
