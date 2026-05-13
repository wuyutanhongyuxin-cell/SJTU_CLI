# DRIVEN-WORKFLOW · codex 自走的 driven 协议

> 等价于 Claude Code 的 `superpowers:subagent-driven-development` 技能。
> 让 codex 在用户**不在线指导**的情况下，**安全**地推进剩余 task。

---

## 核心 4 条原则

### 1. fresh subagent per task（独立 context）

如果 codex 支持子会话 / sub-agent / sub-task：
- 每个 task 派一个**全新**的 subagent，**不继承**前一个 task 的对话历史
- 给 subagent 的 prompt **完整自包含**：任务目标 / 文件清单 / 代码片段 / 验收命令 / 红线摘要 / 报告协议
- subagent 不读 plan 全文，主对话**提取**它需要的段落塞进 prompt

如果 codex 不支持子会话：
- 在每个 task 之间用 `/compact` 或重启 session 清空上下文
- 把上一个 task 的"完成状态 + commit SHA + 影响文件"提炼成 ≤ 100 字的 carry-over 摘要
- 不要让 1 个 session 背 T7+T8+T9 全部历史 —— context 越长，agent 越容易"忘记红线"

### 2. 两阶段 review（spec → quality）

每个 task 实装完成后**强制**走两道关：

**第 1 关 · spec compliance**：
- agent A：读 plan 中该 task 的描述
- agent A：读实装产物（diff / 新文件）
- agent A 输出："✅ 符合 spec" 或 "❌ 偏离：…（列出差异）"
- 偏离 → 派 fix agent 修 → 再 review，直到 ✅

**第 2 关 · code quality**：
- agent B（独立于 A）：只读 diff，不读 plan
- agent B 检查：行数是否超限 / 是否引入新 deps / 是否破坏 envelope / 命名是否一致 / 测试是否真覆盖 / 是否有"防御性垃圾"
- agent B 输出："✅ 质量过关" 或 "❌ 问题：…"

**两关都过才能 commit**。

### 3. trust but verify（主对话亲核）

subagent / 子会话报告"DONE"时，主对话**必须**亲自跑这 4 条：

```powershell
cargo check --all-targets 2>&1 | Select-Object -Last 5
cargo clippy --all-targets -- -D warnings 2>&1 | Select-Object -Last 10
cargo fmt --all -- --check
git diff --stat HEAD~1 HEAD   # 看 commit 影响范围合不合理
Get-ChildItem <改动文件> | ForEach-Object { "$($_.FullName): $((Get-Content $_ | Measure-Object -Line).Lines) lines" }
```

任一不通过 = 派 fix agent 修，不接受 subagent 的"应该是过的"。

**真实事故**：Claude 接手期 T4 阶段 subagent 报告"events.rs 199 行"，主对话 `wc -l` 实测 213 行 —— CRLF vs LF 行数计算差异。如果主对话不亲跑，超限文件就被混过去了。

### 4. 遇阻立即停 + 主对话报告

不要"再试一次"超过 3 次。3 次失败 = 派 codex 报告以下信息：

```
STATUS: BLOCKED
task: T_
尝试 1: <做了什么 / 错在哪>
尝试 2: <…>
尝试 3: <…>
怀疑根因: <…>
建议: <让用户决定 / 让 Claude 介入 / 重新设计>
```

---

## 接力具体顺序

### Phase 1 · T7 验证 + commit（30 分钟）

文件已落地（见 `T7-verify-and-commit.md`）。codex 只需：
1. 跑验证 4 件套
2. 修任何失败（应该没有，因为 Claude 已写好代码）
3. commit

### Phase 2 · T8 文档收尾（1 小时）

需要 codex 实装的内容见 `T8-docs-finalization.md`。涉及：
- `README.md` 加 `sjtu jwc calendar` 段
- `SKILL.md` 加 calendar envelope schema
- `tasks/todo.md` 标 T5 完成
- `tasks/lessons.md` 加 T5 教训
- `CLAUDE.md` 更新"当前阶段"行

### Phase 3 · T9 真机 smoke（用户亲跑）

codex **不执行** T9 —— 4 端日历 import 需要用户在 Google Calendar / Apple Calendar / Outlook / 手机本地各跑一次。
codex 的工作：把 `T9-user-real-machine-smoke.md` 整理成清晰 checklist 交给用户。

### Phase 4 · 完工报告

T9 用户反馈全绿后，codex：
1. 在 `tasks/todo.md` 标 T9 完成 + T5 整体完成
2. 在 `tasks/lessons.md` 补 T9 真机教训（如有）
3. **不 push**，给用户最终一行报告：`STATUS: T5 ALL DONE, ready for user push`

---

## 不允许的"快捷做法"

- ❌ "subagent 报告 OK 我就直接 commit，不亲跑验证" —— 违反 trust-but-verify
- ❌ "我把 T7 + T8 + T9 一次性派一个大 subagent" —— 违反 fresh per task
- ❌ "spec 偏离不大，跳过 review 直接 commit" —— 违反两阶段 review
- ❌ "用 `--no-verify` 跳过 pre-commit hook" —— 违反 CLAUDE.md
- ❌ "我看着代码挺干净，不跑 clippy 了" —— clippy 必须 0 warning

---

## 报告模板（每个 task 必填）

```
STATUS: DONE | DONE_WITH_CONCERNS | NEEDS_CONTEXT | BLOCKED

## 我做了什么
- 文件 X（XX 行，新增/修改）
- 文件 Y（XX 行，新增/修改）

## 验证证据（cv = 我亲跑的命令输出最后几行）
- cv: cargo check ...
- cv: cargo clippy ...
- cv: cargo fmt --check ...
- cv: cargo test --lib ...（如适用）
- cv: wc -l 关键文件
- cv: git log --oneline -1（看 commit）

## 偏离 / 顾虑 / 下一步
- <无 / 或具体>
```
