# T7 · CLI Calendar variant + dispatch — 验证 + commit

> **状态**：代码**已经写好**（Claude 上一轮已落地，未 commit）。codex 只需**验证 + commit**。
> **预计耗时**：≤ 30 分钟。
> **触线条件**：见每个 Step 的"停手判断"。

---

## 0. 前置确认（codex 先跑）

```powershell
git log --oneline -1
# 期望输出：980859f feat(jwc): cmd_calendar handler + fail-soft + envelope dual-mode (T5 Task 6)

git status --short
# 期望输出（仅这两行）：
#  M src/cli/jwc/mod.rs
# ?? src/cli/jwc/calendar_cli.rs
```

**停手条件**：
- HEAD 不是 `980859f` → 已经有人在你之前 commit / pull / merge，**停手报告**
- `git status` 有第 3 个改动文件 → 工作区被污染，**停手报告**
- 看到 `session.json` 或 `tests/fixtures/jwc/*.json` 出现在 `git status` → 立即 `git status` 截屏报告（**触线 R5**）

---

## 1. 看 codex 接手时文件是什么样

跑：

```powershell
Get-Content src/cli/jwc/calendar_cli.rs | Measure-Object -Line
Get-Content src/cli/jwc/mod.rs | Measure-Object -Line
```

**期望**：
- `calendar_cli.rs` ≈ 27 行
- `mod.rs` = **200 行**（卡硬上限！再加一行就违规 → 任何"想加点东西"的念头先放下）

**停手条件**：
- `mod.rs` > 200 行 → **触线 R6**，停手报告
- `calendar_cli.rs` > 200 行 → 同上

---

## 2. 看具体内容是否正确

读两个文件：

```powershell
# 读 calendar_cli.rs（应有 CalendarArgs struct，5 个 #[arg(long)] 字段）
Get-Content src/cli/jwc/calendar_cli.rs

# 读 cli/jwc/mod.rs 关键段：
# - line 21:  mod calendar_cli;
# - line 23:  pub use calendar_cli::CalendarArgs;
# - line ~159: JwcSub::Calendar(CalendarArgs) variant
# - line ~193-197: dispatch arm 调 cmd_calendar
Get-Content src/cli/jwc/mod.rs
```

**期望 CalendarArgs 5 个字段**：
```rust
pub xnm: Option<String>,
pub xqm: Option<String>,
pub to: Option<PathBuf>,
pub no_academic: bool,
pub no_exams: bool,
```

**期望 dispatch arm**：
```rust
JwcSub::Calendar(a) => {
    let client = crate::apps::jwc::Client::connect().await?;
    jwc_cmds::cmd_calendar(&client, a.xnm, a.xqm, a.to, a.no_academic, a.no_exams, fmt)
        .await
}
```

**停手条件**：字段名或 dispatch 签名与上述不一致 → **报告偏离**，不要自己改。

---

## 3. 验证 4 件套（trust-but-verify）

按顺序跑（**不并行**，要看到 cargo check 通过才往下走）：

```powershell
# 3.1 编译
cargo check --all-targets 2>&1 | Select-Object -Last 10

# 3.2 fmt 检查
cargo fmt --all -- --check
# 期望：无输出（exit 0）

# 3.3 clippy（-D warnings 严格模式）
cargo clippy --all-targets -- -D warnings 2>&1 | Select-Object -Last 15

# 3.4 跑既有测试（既有 ical 32 个 + 全仓库其他测试，不能因 T7 破坏任一）
cargo test --lib 2>&1 | Select-Object -Last 20
```

**通过标准**：
- 3.1 末尾 `Finished` 或 `warning: ...` 但无 `error:`
- 3.2 完全无输出
- 3.3 `Finished` 且无 `warning: ` 任何行（除 0 个 warning）
- 3.4 `test result: ok. NNN passed; 0 failed`（NNN ≥ 32 包含 ical 部分）

**任一失败时**：
- 不要自己改代码修 —— 这些文件是 Claude 已 review 过的，理论上不应有错
- 报告失败的具体行 + 看是不是环境差异（如 Rust 版本不一致）
- 在主对话给用户决定

---

## 4. 验证 CLI 实际能跑（runtime smoke）

```powershell
cargo run --release -- jwc calendar --help 2>&1 | Select-Object -First 30
```

**期望**：help 输出包含 5 个 `--` flag：
- `--xnm <XNM>`
- `--xqm <XQM>`
- `--to <TO>`
- `--no-academic`
- `--no-exams`

**停手条件**：
- help 不出 / panic / clap parse error → 报告

**不要**真跑 `sjtu jwc calendar` 拉真实数据 —— 那需要 JAccount session，留给 T9 用户亲跑。

---

## 5. commit

通过 3 + 4 之后，commit：

```powershell
git add src/cli/jwc/calendar_cli.rs src/cli/jwc/mod.rs

git commit -m @'
feat(jwc): expose 'sjtu jwc calendar' CLI subcommand (T5 Task 7)

CalendarArgs (5 flags: --xnm/--xqm/--to/--no-academic/--no-exams) 接入
JwcSub::Calendar variant + dispatch arm；调 T6 已落地的 cmd_calendar handler。

T7 让 T5 主线 7 个实装 task 全部完成（T8 文档 + T9 真机 smoke 收尾）。

Co-Authored-By: codex
'@
```

**注意**：
- **不要** `git push`（**触线 R3**）
- **不要** `git commit --amend`（**触线 R3**）
- commit message 末尾 `Co-Authored-By: codex`（标明非人类作者，符合 CLAUDE.md git 规范）
- **不要**带 emoji
- 不要 `--no-verify`

---

## 6. 完工报告

回主对话给用户：

```
STATUS: DONE
T7: CLI Calendar variant + dispatch
commit: <SHA>
关键证据：
- cargo check: Finished ...
- cargo clippy: 0 warning
- cargo fmt --check: 无输出
- cargo test --lib: NNN passed, 0 failed
- src/cli/jwc/mod.rs: 200 行（卡上限）
- src/cli/jwc/calendar_cli.rs: 27 行
- sjtu jwc calendar --help: 5 个 flag 正常显示
偏离: 无
下一步: 进入 T8 文档收尾
```

---

## 故障排查（如果某步 fail）

### 编译错 "unresolved import `crate::commands::jwc::cmd_calendar`"
- 检查 `src/commands/jwc/mod.rs` 是否含 `pub use ical::handler::cmd_calendar;`
- 这是 T6 commit `980859f` 应已落地的，如果不在 → HEAD 不对

### clippy "needless_pass_by_value" / "redundant_clone"
- 上报，**不要**自己改 calendar_cli.rs / mod.rs 试图消 warning
- 可能是 Claude 写的代码 + 你的 clippy 版本差异

### `cli/jwc/mod.rs` > 200 行
- 报告 + 提议拆分：把 `GpaScopeArg` + `GpaRankArg` + `impl From` 拆到新文件 `src/cli/jwc/gpa_cli.rs`
- **等用户批准**再拆，不自己拆

### help 不出 5 个 flag
- 看 calendar_cli.rs 是否在 `mod.rs` `pub use` 了
- 看 `JwcSub::Calendar(CalendarArgs)` variant 是否在 enum 里
- 报告具体看到的 help 输出
