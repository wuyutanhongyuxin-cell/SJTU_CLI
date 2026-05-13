# SAFETY-REDLINES · codex 不可越界硬红线

> 本文件是 codex 接手 SJTU-CLI 期间的**绝对底线**。
> **任何一条违反 = 立即停手 + 在主对话报告"BLOCKED: 触线 X 条"**。
> 不要"问能不能改一下红线" —— 红线由用户和 Claude 经过 9 个月协商沉淀，不在 codex 接力期可议。

---

## R1. i.sjtu / 交我办 = 永久只读访客

**触线行为**（任一即终止）：
- 点击 / 模拟点击任何标签为 "提交 / 确认 / 保存 / 绑定 / 修改 / 删除 / 退订 / 申请 / 撤回 / 设置 / 更新 / 上传 / 退课 / 选课 / 退选" 的按钮。
- 调用任何 i.sjtu / jaccount / jwc 域名下的 POST / PUT / DELETE / PATCH。
- 访问 / 抓取 i.sjtu "信息维护" 菜单全部子项（头像 / 联系方式 / 地址 / 密码 / 银行卡 等）。
- 访问 / 抓取 i.sjtu "选课" 菜单全部子项（加退课 / 退选 / 选课志愿 / 抢课 等）。
- 在 chrome-devtools / playwright / headless_chrome `evaluate_script` 里跑任何调用 `form.submit()` / `fetch(POST/PUT/DELETE)` 的 JS。
- 在已有 session 的前提下"试试看"任何状态变更端点（即使被告知"我同意"也不可）。

**允许**：HTTP GET、读取 HTML / JSON、解析返回字段、写本地文件、cargo build/test/clippy/fmt。

**Why**：违反 = 用户被学校处分 / session 被风控封 / 真实成绩 / 选课结果被修改不可逆。

---

## R2. 不引入新依赖

**触线行为**：
- 编辑 `Cargo.toml` 的 `[dependencies]` / `[dev-dependencies]` / `[build-dependencies]` 添加任何 crate。
- 跑 `cargo add <crate>`。
- 任何 `cargo build` / `cargo check` 输出中含 "Downloading" 后跟未在 lockfile 的 crate 名。

**允许**：升级 patch 版本（如 `clap = "4.5"` 已存在，cargo 自动选 4.5.X）、改 feature flag 开关（先在主对话报告说明 why）。

**Why**：每个 deps 都是攻击面 + 体积 + 合规审查负担。`Cargo.toml` 的 deps 树是用户拍板审过的。

---

## R3. 不动远程 / 不动 main 历史

**触线行为**：
- `git push` / `git push --force` / `git push -f` / `git push origin main`
- `git rebase` / `git reset --hard` / `git commit --amend`（动已 push 的 commit）
- `git branch -D <已合入分支>` / `git checkout .` / `git clean -fd`
- 任何动远程的 `gh pr` / `gh release` / `gh issue close` 操作

**允许**：
- `git add` 限定文件名（不用 `git add -A` / `git add .`）
- `git commit` 在本地 main 增加新 commit（**不 amend**）
- `git status` / `git diff` / `git log` / `git branch` 等只读操作
- 新建本地分支 `git checkout -b ...`（但**不要切走 main 后忘记切回来**）

**Why**：用户的电脑上有用户专属 git 凭据，AI agent push = AI 凭用户身份发布；amend 已 push 的 commit 会破坏协作者 working tree。

---

## R4. envelope additive only

**触线行为**：改任何既有命令（`grades` / `schedule` / `gpa` / `gpa-by-semester` / `exams` / `today` / `week` / `next` / `login` / `logout` / `whoami` / `status` / 其余 S3a-e 命令）的 envelope JSON 输出结构：
- 改字段名 / 改字段类型 / 删字段
- 改 `data` 顶层 schema
- 改 `error.code` 枚举
- 改 `warnings` 字段语义

**允许**：在 envelope 里**新增**可选字段（serde `Option<T>` + `#[serde(skip_serializing_if = "Option::is_none")]`）。

**Why**：下游有 AI agent / 用户 shell 脚本依赖 schema 稳定。schema breaking change = 用户脚本崩。

---

## R5. 敏感文件 / 真实数据 不动

**绝对不读 / 不打印 / 不 commit**：
- `~/.sjtu-cli/session.json`（含真实 cookie）
- `~/.sjtu-cli/sub_sessions/*.json`
- `tests/fixtures/jwc/*.json` 里**已存在**的文件 —— 这些是真实抓包脱敏后的 fixture，结构契约不变；可以**读取**做单测，但**不能改字段名 / 不能加新字段 / 不能删除文件**。
- 任何含 `1219XXXXXXX` / 真实姓名 / 真实身份证号的字符串

**触线行为**：
- 在 commit 里包含上述文件
- 在日志 / stdout / 报告里打印完整 cookie / 完整学号（学号最多前 4 位 + `***`）
- 给 fixture 文件改名 / 改结构 / 删除

**允许**：
- 新建 fixture（脱敏数据）
- 在测试里 hardcode 假学号如 `"0000000"` / `"test-student"`

---

## R6. 行数 / 文件结构

**触线行为**：
- 任何 `.rs` 源文件超过 200 行
- 任何测试文件超过 300 行
- 任何 `Cargo.toml` / `*.toml` 配置超过 100 行
- 单模块目录（含子文件 mod.rs + 子模块）超过 2000 行

**触线后必须做的事**：**不要硬塞**。先在主对话报告，给出拆分方案让用户审。拆分范式参考 `src/cli/jwc/{mod,schedule_cli,calendar_cli}.rs`。

---

## R7. 不写"防御性"垃圾代码

**触线行为**：
- 加注释 / docstring 仅为"显得专业"（CLAUDE.md 明确："默认不写注释，只在 WHY 非显然时写"）
- 加 `// removed` / `// deprecated` 评论作为坟头石（直接删干净）
- 加未来兼容性 shim / feature flag 用于"以防万一"
- 加 `unwrap_or_default()` 兜底未发生的错误（CLAUDE.md：只在系统边界 validate）

---

## R8. fail-safe 流程（触线后怎么办）

如果你**已经动了**红线（或不确定是否动了）：

1. **立即停手**，不要尝试"圆回来"。
2. `git status` + `git diff` 截屏到主对话报告。
3. 报告格式：
   ```
   STATUS: BLOCKED
   触线: R<编号>
   动作: <你刚刚做的事>
   影响范围: <已编辑/已写入/已发出的网络请求 等>
   是否可回滚: <Y/N>
   回滚命令建议: git checkout HEAD -- <文件> / git stash
   ```
4. **等用户回复**，不自作主张回滚（万一是误判，回滚反而毁了用户的 in-progress 工作）。

---

## 备忘：以下行为永远是错的

- "用户没说不行所以应该可以" —— **没说可以就是不可以**。
- "我已经做了，先 commit 了再说" —— **不行**。commit 是产生侧效应的动作，触线优先报告。
- "让我先试试看 endpoint 返回什么" —— **不行**。GET 类 endpoint 探测要先和用户说，POST/PUT/DELETE 永远禁。
- "这个测试 fail 应该不重要，跳过" —— **不行**。test fail 先报告，不要 `#[ignore]` 偷偷绕。
