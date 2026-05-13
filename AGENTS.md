> **codex / 其它 AI agent 自动加载入口**。
> 与 `CLAUDE.md`（Claude Code 用）并列，内容**互补不冲突**：本文件只写 codex 接力专属的"红线 + 当前状态 + 任务索引"。
> codex 启动后**必须**先读完本文件 + `docs/codex-handoff/SAFETY-REDLINES.md`，再读 `CLAUDE.md` 补充项目宪法。

---

# SJTU-CLI · AGENTS.md（codex 接力交接）

## 1. 你是谁 / 在干什么

你是 **codex**（OpenAI 系），接手 Claude Code 推进到一半的 SJTU-CLI 项目。
**接手原因**：Claude 用户额度耗尽，需要 codex 把 T5 Plan 收尾（剩 T7 验证 commit / T8 文档 / T9 用户亲跑）。

**项目宪法 = `CLAUDE.md`**（适用于所有 AI agent，不只是 Claude）。本文件覆写或扩充其中针对 codex 的部分。

## 2. 项目当前快照（HEAD = `980859f`）

- 分支：`main`
- 已完成：S0..S3f / Canvas Video V5.F；**T5 Plan T1..T6** 已 commit
- 工作区（未 commit）：
  - `src/cli/jwc/calendar_cli.rs`（新建，27 行）— T7 文件
  - `src/cli/jwc/mod.rs`（修改，**200 行 整** ← 卡硬上限，不能再加内容）
- **下一步**：T7 验证 + commit → T8 文档收尾 → T9 用户亲跑 4 端真机 smoke
- 详细任务索引见 `docs/codex-handoff/README.md`

## 3. 不可越界的硬红线（违反 = 立即停手）

> **完整版**：`docs/codex-handoff/SAFETY-REDLINES.md`。**此处只列触发即终止的 5 条**。

1. **i.sjtu / 交我办 = 只读访客**。绝对不点"提交 / 确认 / 保存 / 绑定 / 修改 / 删除 / 退订 / 申请 / 撤回"任一按钮；绝对不触动个人信息（信息维护）与选课菜单。即使用户 session 在握，也按"只读"操作。
2. **不引入新依赖**。`Cargo.toml` 改动 = 主对话先批准。clippy / fmt 不算"新依赖"。
3. **不 `git push`** / 不 `git push --force` / 不动 main 分支历史。**只 commit 到本地**，让用户自己 push。
4. **不动既有命令的 envelope**（grades / schedule / gpa / exams / today / week / next / login 等）。只能**新增**字段，不能改既有字段名 / 类型 / 含义。
5. **不动这些目录 / 文件**（已有真实数据或敏感配置）：
   - `~/.sjtu-cli/`（用户 session）
   - `tests/fixtures/jwc/academic_calendar_*.json`（已落盘 fixture，T6 已用）
   - 任何含 `session.json` / `cookies.json` / 真实学号姓名的文件

## 4. 接力 driven 工作流（codex 自走）

详见 `docs/codex-handoff/DRIVEN-WORKFLOW.md`。**核心 4 条**：

1. **fresh subagent per task**（如 codex 支持 subagent / 子会话）—— 不让一个 agent 同时背 3 个 task 上下文，污染极重。
2. **两阶段 review**：实装 agent → spec compliance 检查 agent → code quality 检查 agent → 通过才 commit。
3. **trust but verify**：subagent 报告"DONE"必须自己跑 `cargo check` + `cargo clippy --all-targets -- -D warnings` + `cargo fmt --all -- --check` + `wc -l` 三件套 **亲自核验**，不信 agent 的口述。
4. **遇阻立即停 + 报告主对话**，不要自作主张大改。`docs/codex-handoff/README.md` 给出每个 task 的"停手判断条件"。

## 5. 行数硬限制（再强调一次）

| 文件类型 | 上限 | 当前接近上限的文件 |
|---|---|---|
| 单 `.rs` 源文件 | **200 行** | `src/cli/jwc/mod.rs` **= 200 行**（卡线，再加就违规） |
| 单测文件 | 300 行 | — |
| 单模块目录（含子文件） | 2000 行 | — |
| 配置文件（`*.toml`） | 100 行 | — |

**T7 commit 之后**，如有任何后续任务需要再动 `cli/jwc/mod.rs`，**先拆 `GpaScopeArg` / `GpaRankArg` 的 impl 到 `src/cli/jwc/gpa_cli.rs`**（参考 `schedule_cli.rs` / `calendar_cli.rs` 已建立的拆分范式）。

## 6. 沟通与汇报协议

完成每个 task 后向用户发送 4 段式报告：

```
STATUS: DONE | DONE_WITH_CONCERNS | NEEDS_CONTEXT | BLOCKED

## 完成的事
- 文件 X（XX 行）+ 文件 Y（XX 行）
- 测试：XX/XX 全过 / clippy 0 warning / fmt 0 diff
- commit SHA: xxxxxx

## 关键命令证据
- cargo check 最后 5 行
- cargo clippy 最后 5 行
- wc -l 关键文件

## 偏离 / 顾虑
（行数是否超 200 / 是否新建了未在 plan 里的文件 / 其他）
```

## 7. 启动 checklist（codex 第一次进项目时跑一遍）

- [ ] `git status` 确认工作区与本文件 §2 描述一致
- [ ] `git log --oneline -10` 确认 HEAD = `980859f` 及最近 5 个 commit
- [ ] `cargo check 2>&1 | tail -5` 确认主分支编译通过
- [ ] `cat docs/codex-handoff/SAFETY-REDLINES.md` 读完红线
- [ ] `cat docs/codex-handoff/README.md` 进入任务索引
- [ ] 按索引顺序：先 T7 → 再 T8 → 最后 T9（T9 = 用户亲跑，codex 只提供 checklist）

## 8. 何时主动停下来问用户

立即停 + 问，不要硬推：

- 任何编译错 / 测试 fail 在 3 次尝试内修不掉
- 需要新建 plan 中未列的文件 / 改 plan 中未列的文件
- 任何文件即将超过行数上限
- 任何 cargo 输出含 "downloading" / "compiling" 新 crate（暗示有依赖偷渡）
- 任何 git 操作可能影响远程（push / fetch + merge / rebase）
- 任何疑似触碰 i.sjtu 写操作的代码（POST / 表单提交 / 状态变更）

---

**最后一句话**：你不是在赶进度，你是在保住别人 6 周的工作不被 1 次 AI 失控弄崩。**慢一点没关系，错一行不行**。
