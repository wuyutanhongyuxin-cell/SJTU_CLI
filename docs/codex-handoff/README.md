# codex 接力 handoff · 索引

> 给 codex（或任何接手的 AI agent）的入口文档。**先读 `/AGENTS.md` 和本目录所有 .md，再开工**。

## 当前状态（HEAD = `980859f`）

- T5 Plan T1..T6 已 commit
- **工作区有未 commit 改动**：T7 文件已落地但未验证 / 未 commit
- 待办：T7 收尾 → T8 文档 → T9 用户亲跑

## 文档清单（**必读顺序**）

| # | 文件 | 用途 | 必读 |
|---|---|---|---|
| 1 | [`SAFETY-REDLINES.md`](SAFETY-REDLINES.md) | 红线完整清单 + 违反时的 fail-safe 流程 | **强制** |
| 2 | [`DRIVEN-WORKFLOW.md`](DRIVEN-WORKFLOW.md) | codex 自走的 driven 协议（fresh subagent / 两阶段 review / trust-but-verify） | **强制** |
| 3 | [`T7-verify-and-commit.md`](T7-verify-and-commit.md) | T7 验证 + commit（文件已落地，只剩 cargo check + clippy + fmt + test + commit） | 第一个跑 |
| 4 | [`T8-docs-finalization.md`](T8-docs-finalization.md) | T8 文档收尾（README + SKILL + lessons + todo + CLAUDE 阶段） | T7 通过后 |
| 5 | [`T9-user-real-machine-smoke.md`](T9-user-real-machine-smoke.md) | T9 用户亲跑（4 端日历真机 smoke）— codex 只提供 checklist，**不执行** | T8 通过后给用户 |

## 任务顺序图

```
codex 启动
  → 读 /AGENTS.md + 本目录全部 5 个 .md
  → 跑 T7 验证 + commit（30 分钟内应完成）
  → 跑 T8 文档收尾（1 小时内应完成）
  → 给用户 T9 checklist（**用户亲手跑 4 端日历**，codex 等结果）
  → T9 报告全绿后，最终向用户报告 T5 Plan 全部完成
```

## 何时停手 + 找用户

参见 `/AGENTS.md §8`。简版：

- 编译错 / 测试 fail 在 3 次尝试内修不掉
- 行数即将越限
- 任何 cargo 在下载新 crate
- 任何 git 操作影响远程
- 任何疑似 i.sjtu 写操作的代码

## 报告模板

每完成一个 task 回复用户：

```
STATUS: DONE
T_: <task 名>
commit: <SHA>
关键证据：
- cargo check: <最后 5 行>
- cargo clippy: <最后 3 行 + warning 数>
- cargo fmt --check: <空行 / "All good">
- wc -l 关键文件
偏离：<无 / 或具体>
下一步：<进入 T_+1>
```
