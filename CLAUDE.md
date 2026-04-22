# CLAUDE.md — SJTU-CLI 项目规范

> 本文件是 SJTU-CLI 项目的 Claude Code 指令，基于"通用 AI 编程规范"填充项目专属区域。
> Claude Code 每次启动自动读取。

---

## [项目专属区域]

### 项目名称
**SJTU-CLI** —— 上海交通大学"交我办"命令行工具

### 一句话描述
给 SJTU 师生用的终端 CLI，通过 JAccount 扫码登录后，一行命令查课表 / 成绩 / 一卡通 / 通知 / Canvas，输出支持 YAML/JSON，方便 AI Agent 调用。

### 技术栈（Rust 实现）
- **语言**：Rust（stable, edition 2021）
- **包管理 / 构建**：cargo
- **CLI 框架**：clap 4（derive API）
- **HTTP 客户端**：reqwest（features: `cookies` / `json` / `rustls-tls` / `gzip`）
- **异步运行时**：tokio（`rt-multi-thread` + `macros`）
- **浏览器自动化**：headless_chrome（纯 Rust CDP 驱动）
- **Cookie 提取兜底**：rookie（browser-cookie3 的 Rust 等价）
- **HTML 解析**：scraper（html5ever）
- **QR 终端显示**：qrcode + ANSI 色块
- **序列化**：serde + serde_json + serde_yml + toml
- **表格 / 颜色**：comfy-table + owo-colors
- **TTY 检测**：is-terminal
- **时间**：chrono（含 `serde` feature）
- **路径**：directories（跨平台 `~/.sjtu-cli/` 定位）
- **金额**：rust_decimal（一卡通消费金额，禁用 f64）
- **错误**：anyhow（bin 层）+ thiserror（lib 层）
- **日志**：tracing + tracing-subscriber
- **测试**：内置 `#[test]` + tokio-test + mockito
- **Lint**：cargo clippy + cargo fmt
- **CI**：GitHub Actions（stable × windows / ubuntu / macos）

### 项目结构
```
sjtu-cli/
├── Cargo.toml                       # 依赖 + 构建配置
├── Cargo.lock
├── rust-toolchain.toml              # 固定 stable
├── README.md
├── SKILL.md                         # AI Agent 使用指南
├── SCHEMA.md                        # 输出信封契约
├── CLAUDE.md                        # 本文件
├── .env.example
├── config.example.toml
├── src/
│   ├── main.rs                      # 二进制入口，调用 cli::run
│   ├── lib.rs                       # 模块 re-export；VERSION 常量
│   ├── cli.rs                       # clap 命令枚举 + 派发
│   ├── config.rs                    # ~/.sjtu-cli/ 路径管理
│   ├── cookies.rs                   # Cookie TTL / 加载 / 保存
│   ├── error.rs                     # thiserror 统一异常
│   ├── output.rs                    # Envelope + TTY 检测 + YAML/JSON/Table 切换
│   ├── auth/                        # 登录模块
│   │   ├── mod.rs
│   │   ├── qr_login.rs              # ★ headless_chrome QR 扫码
│   │   ├── browser_extract.rs       # rookie cookie 兜底
│   │   └── cas.rs                   # 子系统 CAS 跳转
│   ├── apps/                        # 每个子系统一个文件
│   │   ├── mod.rs
│   │   ├── jwc.rs                   # 教务（课表/成绩）
│   │   ├── card.rs                  # 一卡通
│   │   ├── notifications.rs         # 交我办通知
│   │   └── canvas.rs                # Canvas LMS
│   ├── commands/                    # CLI 子命令实现
│   │   ├── mod.rs
│   │   ├── auth_cmds.rs             # login/logout/status/whoami
│   │   ├── schedule.rs              # schedule/today/week/next
│   │   ├── grades.rs                # grades/gpa
│   │   ├── card.rs                  # card/card history
│   │   ├── notifications.rs
│   │   └── canvas.rs
│   └── models/                      # struct（derive Serialize/Deserialize）
│       ├── mod.rs
│       ├── course.rs
│       ├── grade.rs
│       ├── card_record.rs
│       └── notification.rs
├── tests/                           # 集成测试
├── tasks/
│   ├── todo.md                      # 任务清单
│   └── lessons.md                   # 经验积累
└── .github/workflows/ci.yml
```

### 当前阶段
- **已完成**：S0 骨架 / S1 QR 扫码登录（实战验证抓到 JAAuthCookie）
- **下一步**：S2 — CAS 子系统跳转
- **详细进度**：见 `tasks/todo.md`
- **经验总结**：见 `tasks/lessons.md`

### 项目专属约束
- **合规第一**：只做读操作；不做抢课 / 代登录 / 批量爬他人
- **用户隐私**：`~/.sjtu-cli/` 权限 600（Unix）/ 仅当前用户可读（Windows ACL 可留 TODO）；日志脱敏学号姓名
- **Cookie 安全**：不提交任何真实 cookie；`.gitignore` 必须含 `session.json`、`sub_sessions/`
- **金额**：一卡通消费金额必须用 `rust_decimal::Decimal`，绝不用 f32/f64
- **多子系统适配**：每个 SJTU 子系统（jwc / card / canvas / library）独立一个 `apps/*.rs`，只改一处
- **参考范式**：主参考 `xiaohongshu-cli` 的三级认证 + Envelope（跨语言移植）；辅参考 `bilibili-cli` 的 async 结构

---

## 开发者背景

我不是专业开发者，使用 Claude Code 辅助编程。请：
- 代码加中文注释，关键逻辑额外解释
- 遇到复杂问题先给方案让我确认，不要直接大改
- 报错时解释原因 + 修复方案，不要只贴代码
- 优先最简实现，不要过度工程化
- Rust 初学者友好：遇到生命周期 / trait bound 报错时讲清楚原理

---

## 上下文管理规范（核心）

### 1. 文件行数硬限制

| 文件类型 | 最大行数 | 超限动作 |
|----------|----------|----------|
| 单个源代码文件（.rs） | **200 行** | 立即拆分为多个文件 |
| 单个模块目录（含 mod.rs 与所有子文件） | **2000 行** | 拆分为子模块 |
| 测试文件 | **300 行** | 按功能拆分测试文件 |
| 配置文件（Cargo.toml / *.toml）| **100 行** | 拆分为多个配置文件 |

**每次创建或修改文件后，检查行数。接近限制时主动提醒我。**

### 2. 每个目录必须有 README.md（当目录下有 3 个以上文件时）

内容模板：
```markdown
# 目录名

## 用途
一句话说明这个目录做什么。

## 文件清单
- `xxx.rs` — 做什么（~行数）
- `yyy.rs` — 做什么（~行数）

## 依赖关系
- 本目录依赖：xxx 模块
- 被以下模块依赖：yyy
```

### 3. 定期清理（每 2-3 天新功能开发后执行一次）

当我说 **"清理一下"** 时：

1. **行数审计**：列出所有超过 150 行的文件，建议拆分
2. **死代码检测**：`cargo +nightly udeps` 或手工找未 `use` / 未 `pub use` 的函数
3. **TODO 清理**：列出所有 `// TODO` / `// FIXME` / `// HACK`
4. **一次性脚本**：找出临时 `examples/*.rs`、单次运行的 bin，建议删除
5. **描述同步**：检查 CLAUDE.md 项目结构 vs 实际目录
6. **依赖检查**：`Cargo.toml` 无未使用依赖（`cargo-machete` 或手动）

---

## Sub-Agent 并行调度规则

**并行派遣**（全部满足）：
- 3+ 不相关任务
- 不操作同一文件
- 无输入输出依赖

**顺序派遣**（任一触发）：
- B 需要 A 的输出
- 同文件（合并冲突风险）
- 范围不明

**后台 Agent**：研究/分析类（不改文件）后台跑，不阻塞主对话。

---

## 编码规范

### 错误处理
- 所有外部调用（HTTP、文件 IO、进程调用）必须返回 `Result<T, E>`
- 库层错误用 `thiserror` 定义精确 variant；bin 层用 `anyhow` 收口
- 失败时 graceful degradation：友好提示 + 缓存/默认值
- `tracing` 记录详情；对用户只返回 Envelope `error.message`，不暴露 stack / 内部路径

### 函数设计
- 单个函数 ≤ 30 行（超了就拆）
- 动词开头：`fetch_schedule()` / `parse_grade_table()`
- 每个 `pub fn` 有 doc comment（`///`），说明输入输出和可能的 error variant
- 尽量不用 `.unwrap()` / `.expect()`；必须用时写明为什么不会 panic

### 依赖管理
- **不要自行引入新依赖**，需要新 crate 时先问我
- 优先 std，其次已声明依赖
- 新增依赖立即更新 `Cargo.toml`，并说明 feature flag 选择
- 拒绝同时引入功能重叠的 crate（比如已有 reqwest 就不要再加 ureq）

### 配置管理
- 敏感信息（JAccount cookie）放 `~/.sjtu-cli/session.json`（Unix 权限 600）
- 非敏感配置放 `config.toml`
- 绝不硬编码 cookie / 学号 / 密钥

---

## Git 规范

### Commit 信息格式
```
<类型>: <一句话描述>
类型：feat | fix | refactor | docs | chore
```

### Commit 前检查
- 没有 `.env` / `session.json` / `sub_sessions/` / `target/` / `*.log`
- 代码能编译（至少 `cargo check` 不报错）
- 没有任何真实 cookie / 学号 / 姓名
- `cargo fmt --check` 通过

---

## 沟通规范

### AI 不确定时
- **直接说不确定**，不编造
- 给 2-3 方案让我选
- 标明优缺点

### 任务太大时
- 不一口气全做完
- 先给 5-8 步拆分计划让我确认
- 每完成一步告诉我进度

### 代码出问题时
1. 是什么问题（一句话）
2. 为什么（原因分析，含 Rust 特性层面的解释如适用）
3. 修复方案
- 不要只说"我来修一下"然后默默改一堆

### 关键词触发

| 我说 | 你做 |
|------|------|
| "清理一下" | 执行定期清理流程 |
| "拆一下" | 检查指定文件/模块，给拆分方案 |
| "健康检查" | 完整项目健康度检查（cargo check + clippy + fmt + 行数审计）|
| "现在到哪了" | 总结进度，参考 `tasks/todo.md` |
| "省着点" | 减少 token：简短、不重复输出完整文件 |
| "全力跑" | 可并行、大改、不逐步确认 |

---

## 性能优化（省钱 + 保持 AI 聪明）

### Token 节省
1. 只输出变更部分，不重复整个文件
2. 简单问题不贴全部相关代码
3. 长文件只输出相关函数
4. 用 `// ... existing code ...` 标记未修改

### 上下文保鲜
1. 对话 > 20 轮建议 `/compact`
2. 切换模块建议新 session
3. 大量探索用 sub-agent
4. Debug 用 Explore sub-agent 搜索

### 何时开新 Session
- 当前 > 30 轮
- 切到完全不同模块
- AI 回复质量下降（前后矛盾）
- 大型重构

---

## 项目文件模板

### 新模块 Checklist
- [ ] 目录级 README.md（文件数 ≥ 3 时）
- [ ] `mod.rs` + 子文件
- [ ] 每个文件 module doc comment（`//!`）+ 中文注释
- [ ] 行数 < 200
- [ ] 更新 CLAUDE.md 项目结构
- [ ] 更新 `tasks/todo.md`

### 新功能 Checklist
- [ ] `Result<T, E>` 错误处理
- [ ] 有缓存策略（若调外部 API）
- [ ] 不引入新依赖（或已批准）
- [ ] 文件行数未超限
- [ ] 能独立 `cargo test` 测试

---

## SJTU-CLI 专属提醒

### 登录相关
- **任何时候不要打印完整 cookie**，日志只打前 8 位 + `***`
- QR 登录失败时给用户 3 种降级：重试 / `--browser` 走 rookie 提取 / 手动粘贴
- 子系统 session 过期时自动走 CAS 刷新，不要弹框要求用户重新扫码

### 子系统相关
- 教务处（jwc）返回 HTML 可能改版：解析失败时 Envelope 附 `error.raw_snippet` 而非崩溃
- 一卡通金额：`rust_decimal::Decimal`，序列化为字符串（避免 JSON 的 f64 精度坑）
- Canvas 优先使用官方 REST API + Personal Access Token，次选 OAuth2；不优先页面爬取
- 通知去重按 `(source, notification_id)` 复合键

### 测试相关
- 单元测试用 `mockito` 启本地 HTTP server，不真打 SJTU 服务器
- 真实 API 测试加 `#[ignore]`，`cargo test -- --ignored` 才跑
- CI 默认 `cargo test`（跳过 ignored）
