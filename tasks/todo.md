# SJTU-CLI Todo

> 任务清单，按阶段组织。完成一项勾选一项。
> 每个阶段完成后 checkpoint 汇报。
> 详细规划见 `../SJTU-CLI规划.md`（在上层目录）。
>
> **2026-04-22 更新：技术栈从 Python 切到 Rust**，下方清单已同步。

---

## ✅ S0 — Skeleton + 配置体系（已完成 2026-04-22）

预估 0.5 天。目标：最小可运行骨架，`cargo run -- --help` 能跑。

**前置：** 确认本机已装 Rust 工具链（`rustc --version` 能输出）。→ cargo 1.95.0（winget 装 rustup 后就位）。

- [x] 创建 `Cargo.toml`（`[package]` + `[[bin]] name = "sjtu"` + 依赖）
- [x] 创建 `rust-toolchain.toml`（锁 stable）
- [x] 创建 `.gitignore`（`target/`、`session.json`、`sub_sessions/`、`.env`、`*.log`）
- [x] 创建 `.env.example` / `config.example.toml`
- [x] 创建 `src/main.rs`（ExitCode 包装，调 `sjtu_cli::cli::run`；tokio runtime 留给 S1）
- [x] 创建 `src/lib.rs`（`pub mod` 声明 + `VERSION` 常量）
- [x] 创建 `src/cli.rs`（clap `#[derive(Parser)]` + `Commands` 枚举，含占位 `hello` 子命令）
- [x] 创建 `src/config.rs`（`directories::ProjectDirs` → `~/.sjtu-cli/`，`ensure_dirs()`）
- [x] 创建 `src/cookies.rs`（`Session` struct + serde_json 读写 + `is_expired()` + 8 字符脱敏）
- [x] 创建 `src/output.rs`（`Envelope<T>` 泛型 + `OutputFormat` 枚举 + TTY 检测；Table 暂退回 YAML）
- [x] 创建 `src/error.rs`（thiserror `SjtuCliError`：6 个 variant + `code()` 映射 snake_case）
- [x] 验证：`cargo build` 通过（1m09s，0 warning）；`cargo clippy -D warnings` 通过；`cargo fmt --check` 通过
- [x] **Checkpoint：`sjtu hello --yaml` / `--json` / 默认（管道自动 YAML）均输出合法 Envelope**

**S0 留白（进 S1 前不阻塞，但要记账）：**
- Table 渲染暂退回 YAML（待 S3 引入 `comfy-table`）
- Windows ACL 收紧（目前只有 Unix cfg 下 chmod 600/700）
- `tracing` 未接入（S1 登录流程开始用）
- 尚无 `tests/` 目录（S6 集中补）

---

## ✅ S1 — ★ QR 扫码登录（已完成 2026-04-22）

预估 1-2 天。目标：跑通 `sjtu login` 扫码并保存 `session.json`。
**plan 文件：** `C:\Users\16191\.claude\plans\bubbly-bubbling-firefly.md`

- [x] 添加依赖：`headless_chrome` 1 / `qrcode` 0.14 / `image` 0.25 / `rqrr` 0.7 / `rookie` 0.5 / `tracing` 0.1 / `tracing-subscriber` 0.3
- [x] 创建 `src/auth/mod.rs`（Backend 枚举 + `login(Backend)` 入口）
- [x] 创建 `src/auth/qr_login.rs`（headless_chrome 主链路，144 行）
  - [x] `Browser::new(LaunchOptions { headless: false, idle_browser_timeout: 600s })`
  - [x] `browser.new_tab()` → `tab.navigate_to("https://jaccount.sjtu.edu.cn/jaccount/")`（`wait_for_initial_tab` 已 deprecated）
  - [x] 多 selector 探测 QR 元素（canvas / img.qr-img / #qr-img / .qr）
  - [x] `tab.capture_screenshot(Png, …)` 截全屏
  - [x] 用 `rqrr` 解码 + `qrcode` 重绘终端 ANSI 半块（best-effort，失败不阻断）
  - [x] 轮询 `tab.get_url()` == `my.sjtu.edu.cn/ui/app/`，超时 240s
  - [x] `tab.get_cookies()` 过滤 `.sjtu.edu.cn` 域、必含 `JAAuthCookie`
  - [x] 写入 session.json（复用 S0 `cookies::save_session`）
- [x] 创建 `src/auth/qr_render.rs`（image + rqrr + qrcode 三件套，含 2 个单测）
- [x] 创建 `src/auth/browser_extract.rs`（rookie 兜底：Chrome → Edge → Firefox 顺序探测）
- [x] 创建 `src/commands/auth_cmds.rs`：`cmd_login` / `cmd_logout` / `cmd_status`（`whoami` 推迟到 S2）
- [x] 改 `src/cli.rs`：加 `Login { --browser chrome|rookie }` / `Logout` / `Status` 三个 variant
- [x] 改 `src/main.rs`：`tracing_subscriber` 初始化（默认 warn，`RUST_LOG=debug` 可打开）
- [x] 自动验证：build / clippy `-D warnings` / fmt --check / `cargo test`（2 passed）全绿
- [x] 自动验证：`sjtu status` 未登录 → `not_authenticated` envelope 且 exit 0
- [x] 自动验证：`sjtu logout` 幂等 → `cleared: false`
- [x] **人工验证**：`sjtu login` 扫码成功 → 抓到 7 条 SJTU cookie 含 `JAAuthCookie`（前 8 位脱敏 `xxxxxxxx`）；`sjtu status` 读出 `authenticated: true / is_expired: false`，5 条脱敏展示
- [ ] **人工验证（可选）**：`sjtu login --browser rookie` 兜底链路 —— 留给用户日后 Chrome 真启不起来时再验
- [x] **Checkpoint**：S1 主链路在 Windows 11 cmd + Chrome 上跑通

**修复的 bug（实战暴露）：**
1. `LOGIN_URL` 原本指 `jaccount.sjtu.edu.cn/jaccount/`（欢迎页，无 QR）→ 改为 `my.sjtu.edu.cn/ui/app/` 让 CAS 自动跳到带 QR 的真正登录页
2. `tab.get_cookies()` 只看当前 URL 域，扫码完跳回 my.sjtu 时 jaccount 域的 `JAAuthCookie` 抓不到 → 换 CDP `Network.getAllCookies` 跨域抓
3. 抓 cookie 前补 500ms sleep，给 `Set-Cookie` 写入 cookie jar 留时间

**S1 留白：**
- 终端 ANSI QR 分辨率取决于截屏中 QR 实际像素 —— 实测扫不出，fallback 到浏览器窗口扫；可考虑 S2 后改为：拦截 JAccount 的 `qrCode/random` API 直接拿 QR 字符串
- `Session::redacted()` 用 `HashMap<&str>` 以 name 为 key，同名不同域 cookie 会在 status 展示里被覆盖（login `cookie_count: 7` vs status 列 5 条）—— 只影响展示，session.json 内容完整。S2 真用 cookie 时若有歧义再改成 `(name, domain)` 复合 key
- Windows ACL 收紧 session.json 仍未做（继承自 S0）
- 没补 `tests/` 集成测试（按计划留 S6）

---

## ✅ S2 — CAS 子系统跳转（已完成 2026-04-22）

预估 1 天。目标：通用函数 `cas_login(name, target_url)` 能给任意子系统拿 session。

- [x] 加依赖：`reqwest` 0.12（cookies/json/rustls-tls/gzip）+ `tokio` 1（rt-multi-thread/macros）+ `url` 2；dev：`mockito` 1
- [x] `src/main.rs` 改 `#[tokio::main(flavor = "multi_thread", worker_threads = 2)]`；`cli::run` 改 `async`
- [x] `src/cookies.rs` 加 `sub_session_path` / `load_sub_session` / `save_sub_session` / `clear_sub_session`；含路径注入防御（禁 `.` / `/` / `\` / 空格）
- [x] 创建 `src/auth/cas/`（拆 3 文件以守住 200 行硬限）：
  - [x] `mod.rs`（194 行）：`cas_login` 主入口 + `follow_redirect_chain` 手动跟 302 链 + `is_redirect` / `is_jaccount_host` helpers
  - [x] `client.rs`（50 行）：`build_client` 注入主 session 所有 SJTU 域 cookie（含 JAAuthCookie）→ `reqwest::Client` with `redirect::Policy::none()`
  - [x] `tests.rs`（108 行）：mockito 模拟 3 跳 redirect 链验 cookie 累加；模拟 redirect loop 验 10 跳超限报错；非法 URL 测试
- [x] `src/cli.rs` + `src/commands/auth_cmds.rs`：加 `sjtu test-cas <url> --name <n>`（`#[command(hide = true)]`，S3 接入教务后删）
- [x] **实现要点**：手动跟 302 而非 reqwest 默认 follow —— 才能逐跳收 `Set-Cookie`，且落点停在 jaccount 域时立即报 `SubSystemUnreachable`（识别 JAAuthCookie 过期 / 需交互授权）
- [x] 自动验证：build / clippy `-D warnings` / fmt --check / `cargo test`（8 passed = 原 3 + sub_session 路径防御 + 3 mockito + redirect 分类 + jaccount host 判断）
- [x] **Checkpoint 实测**（真实 SJTU 教务 SP `i.sjtu.edu.cn/xtgl/index_initMenu.html`）：
  - 首次 CAS 跳转：`from_cache=false, elapsed_ms=19420, cookie_count=2`（JSESSIONID + keepalive）
  - 第二次同命令：`from_cache=true, elapsed_ms=6`（缓存命中，3200× 加速）
  - sub_session 文件：`%APPDATA%\sjtu-cli\sub_sessions\jwc.json`

**S2 留白：**
- 真实 SJTU 教务的 CAS 落点 URL 是 `login_slogin.html`（而非想象的 `index_initMenu.html`）—— 已在 sub_session 里，S3 jwc 模块用时要按这个落点继续。归属 S3 的调研工作，不在 S2 范围
- `test-cas` 隐藏调试子命令，S3 引入正式 `sjtu schedule` / `sjtu grades` 后删
- 未测：最终 URL 停在 jaccount 域时的报错路径（需要制造 JAAuthCookie 过期场景；可手动 `sjtu logout` 后试）
- `Cargo.toml` 多加了一个 `[dev-dependencies] tokio = "1" { ..., "test-util" }` —— 和生产 tokio 同 crate 不同 features；cargo 会 union，实际生产也会带上 test-util（无害但略肿 30KB），S6 做 tests 优化时可清理

---

## 🟡 S3 — Claude 可操作子系统（S3a–S3e）

> **路线图调整（2026-04-23）**：原 S3/S4/S5 的"教务 + 一卡通 + Canvas"收到用户指示后整体后移到 Phase 2，S3 改为与"让 Claude 直接 CLI 操作阅读"强相关的 5 个子系统：
>
> S3a 水源社区 → S3b 消息中心 → S3c 日程 → S3d 办事大厅 → S3e 生活服务（一卡通余额 / 宿舍电费）。
>
> 每个子阶段的详细设计、入口 URL、端点、Checkpoint 见 `tasks/plan-next.md`。

### 🟡 S3a — 水源社区 shuiyuan.sjtu.edu.cn（只读代码已写，真实 checkpoint 未跑）

**Plan 文件**：`C:/Users/16191/.claude/plans/bubbly-bubbling-firefly.md`

**已完成（代码层）：**

- [x] 依赖：复用 reqwest / tokio / mockito；无需新 crate
- [x] 创建 `src/auth/oauth2/{mod,follow,tests}.rs`（OAuth2 通道，手动跟 302 链）
  - [x] `oauth2_login(name, start_url)` 主入口：跑 JAccount OAuth2 链路 → 落盘 `sub_sessions/<name>.json` 并返回 `OAuth2Result { session, from_cache, elapsed_ms, via_rookie_fallback, final_url }`
  - [x] `follow_redirect_chain` 复用 S2 做法：`Policy::none()` + 手动循环 + `(name,domain,path)` 三元组去重
  - [x] `MAX_REDIRECT_HOPS = 12`
  - [x] 落点停在 JAccount 域时报错（cookie 失效 / 需用户交互）
- [x] 创建 `src/apps/shuiyuan/{mod,api,http,models,render,throttle,tests}.rs`
  - [x] `Client::connect()` = `oauth2_login` + 注入 cookie + 构造节流 reqwest Client
  - [x] 端点：`latest_topics(page, limit)` / `topic(id, post_limit)` / `notifications(unread_only, limit)` / `search(q, scope)` / `current_user()`
  - [x] `current_user()` 特殊：404 返 `Ok(None)`（未登录是合法语义）
  - [x] UA 伪装成 Chrome/124（Discourse 见 curl UA 会 403）
  - [x] `throttle`：300 ms 固定间隔（Discourse 限流 200 req/min + 50 req/10s）
- [x] 创建 `src/commands/shuiyuan.rs`：`cmd_latest` / `cmd_topic` / `cmd_inbox` / `cmd_search` / `cmd_login_probe`
- [x] `src/cli.rs` 加 `Shuiyuan { sub: ShuiyuanSub }` 子命令，含 5 个 variant
- [x] 测试：mockito 模拟 3 跳 OAuth2 redirect 链（3 个测试），复用 S2 的 CAS mockito 范式
- [x] `cargo fmt --check` / `cargo clippy -- -D warnings` / `cargo test` → **25/25 passed**（2026-04-23 修 `bare_client` 代理继承问题后全绿）

**未完工（S3a 继续项）：**

- [x] **CP-1 真实 checkpoint**：`sjtu shuiyuan login-probe` → `authenticated: true` / `from_cache: true` / `elapsed_ms=6` / `current_user.id=72509` — 2026-04-25 真机
- [x] **CP-2 真实 checkpoint**：`sjtu shuiyuan latest --limit 3 --yaml` → `returned=3`，每条 topic 含 id/title/posts_count/views — 2026-04-25 真机
- [x] **CP-3 真实 checkpoint**：`sjtu shuiyuan topic 468808 --post-limit 5 --yaml` → `posts[0].post_number=1` / `username=Narrenschiff` / body 非空（1070 楼帖）— 2026-04-25 真机
- [x] **CP-4 真实 checkpoint**：`sjtu shuiyuan inbox --unread-only --yaml` → `returned=6`，含 `notification_type` / `topic_id` 字段 — 2026-04-25 真机
- [x] **CP-5 真实 checkpoint**：`sjtu shuiyuan search "jaccount" --in post --yaml` → `posts_count=50`，含 topics 数组完整字段 — 2026-04-25 真机
- [x] **CP-6 二次 login-probe**：`from_cache: true` / `elapsed_ms=6 < 100` 缓存命中加速 — 2026-04-25 真机
- [x] 删掉隐藏命令 `sjtu test-cas`（S2 过渡用，S3 起不再需要）— 2026-04-24 收尾
- [ ] S3a 写操作（默认 `--confirm` 二次确认）：`reply <topic_id> --body <...>` / `like <post_id>` / `new-topic --category <...> --title <...> --body <...>`
  - 先拿到 CSRF token（`GET /session/csrf.json` → `{"csrf":"..."}`）
  - 写操作路径 + body 参考 Discourse 官方 openapi，错开 `--confirm`
- [x] **S3a Checkpoint 汇总**：CP-1..CP-6 全绿（2026-04-25），写操作 reply/like/delete-* 真机已 2026-04-24 验过；剩 new-topic CP-W4 真机未触发

### ⚪ S3b — 消息中心

- [x] 调研水源 PM 端点（Discourse `/topics/private-messages{-sent,-unread,-new}/{user}.json` + `/t/<id>.json` 复用 TopicDetail）— 2026-04-24 curl 真机侦察 `target/pm_*.json`
- [x] 代码实装：水源 PM 只读（`sjtu shuiyuan messages --filter <inbox|sent|unread|new>` + `sjtu shuiyuan message <id>` 复用 `cmd_topic`）— 2026-04-24 完成，35 单测绿
  - 新增 `apps::shuiyuan::PmFilter` 枚举 + `Client::messages(filter, page, limit) -> (username, TopicList)`（内部先拉 `/session/current.json` 取用户名）
  - clap：`shuiyuan::PmFilterArg` + `Messages {filter, page, limit}` / `Message {id, post_limit, render}`（后者 dispatch 层直转 `cmd_topic`）
  - 单测：`pm_filter_path_segments_are_correct`（4 个 URL 段映射）+ `parse_pm_inbox_topic_list`（带 `archetype=private_message` 的 TopicSummary 反序列化）
- [x] **CP-M1 真实 checkpoint**：`sjtu shuiyuan messages --filter inbox --yaml` → `username=<水源昵称>` / `returned=2` / 含 PM topic id+title — 2026-04-25 真机
- [x] **CP-M2 真实 checkpoint**：`sjtu shuiyuan message 404691 --post-limit 3 --render {plain|markdown|raw} --yaml` 三模式语义验证：plain 剥 markdown 标记（`[details=...]` → `details="..."`、链接降级）；markdown/raw 保留原始 markdown — 2026-04-25 真机
- [x] S3b 写端点：`sjtu shuiyuan pm-send <username> <title> <body>`（POST `/posts.json` + `archetype=private_message` + **`target_recipients=...`**，水源魔改字段名，非标准 Discourse 的 `target_usernames`），默认 `--yes` — 2026-04-26 真机 CP-PM1 通过：自发自收 topic 469487 → sent 视图返回 → archive 清理。代码 fix：`api_write.rs::pm_send` 字段名 + `tests_write.rs` mockito 断言同步
- [ ] S3b 补端点：`sjtu shuiyuan archive-pm <topic_id>`（PUT `/t/<id>/archive-message.json`）—— 因为 PM 不能 `delete-topic`（水源 + Discourse 都对 PM 静默 no-op），需要正经的 archive 命令收尾
- [ ] 调研"交我办"消息中心 SP URL（`my.sjtu.edu.cn` 的 messages 模块，需用户 QR 扫码配合 chrome-devtools MCP）— 留在 S3b 后半段

### ✅ S3c — Canvas 作业 DDL（2026-04-24 完成 MVP）

> **2026-04-24 Scope 收紧**：用户钦定 S3c 从"交我办日程"改为"Canvas 作业 DDL"；原日程 / jwc 课表 / 聚合日历留 Phase 2。详细契约见 `tasks/s3c-canvas-planner.md` + `tasks/plan-next.md` §S3c。

- [x] 调研：chrome-devtools MCP 实抓 `oc.sjtu.edu.cn` → `tasks/s3c-canvas-planner.md` 写完 6 节（链路 / 端点契约 / 顺带捕获 / 实装建议 / 回写清单 / 元数据）
- [x] `tasks/plan-next.md` §S3c 占位整段替换为实契约（7 条回写点全落位：标题 / 调研打勾 / 端点 / CLI / 文件清单 / Checkpoint / 依赖预报）
- [x] 代码落地 `src/apps/canvas/{mod,api,http,models,throttle,auth,tests_parse,README}` + `src/commands/canvas/{mod,data,handlers}` + `src/cli/canvas.rs` + 顶层 dispatch 注册；全部文件 < 200 行（最长 handlers.rs 188）
- [x] 鉴权：PAT 落盘独立文件 `<config_dir>/sub_sessions/canvas_token.txt`（不污染 `Session` struct）；`SjtuCliError` 新增 `CanvasApi` + `CanvasTokenInvalid` 两个 variant
- [x] `cargo check` / `cargo test --lib` = **53/54 passed**（新增 6 个 Canvas 单测：user/profile 解析 + merge 逻辑 + planner_items assignment+note 混合 + Submissions::is_outstanding 语义 + throttle 间隔）
- [x] **CP-C1** `sjtu canvas setup` + `sjtu canvas whoami --yaml` → `login_id=<脱敏>` + `time_zone=Asia/Shanghai` + `effective_locale=zh-Hans` ✓
- [x] **CP-C2** `sjtu canvas today --yaml` → `date_local=2026-04-24`、`returned=0` / `total_raw=0`（今日无 DDL，符合预期）✓
- [x] **CP-C3** `sjtu canvas upcoming --days 14 --yaml` → 返 2 条 DDL（2026-04-25 23:59 日语演讲稿 / 2026-05-02 23:59 日汉互译），UTC→本地 +08:00 换算正确、`asc` 排序正确 ✓
- [x] **CP-C4** 改 PAT 为无效值后跑 `whoami` → `Canvas PAT 无效或已过期。请重新运行 \`sjtu canvas setup\` 生成新 token。`（Envelope error.code 映射到 `session_expired`）✓

**S3c 留白 / 下阶段备选：**
- CLI 输出现在走 anyhow bin-layer 的 `error: ...` 文本而非 Envelope 错误信封 —— 与 jwbmessage / shuiyuan 同口径，回写 Envelope 错误路径留给 S6 统一处理
- `planner/items` 未处理 Link header 分页（MVP 默认 per_page=100，单页够用；未来 N 天 DDL 通常 ≤ 50 条）
- iCal 订阅路径 `profile.calendar.ics` 已收集但未实装解析（需引 `icalendar` crate，留给"聚合日程"类命令）

### ⚪ S3d — 办事大厅

- [ ] 调研 "办事大厅" SP URL + 待办 / 已办 / 可发起事项接口
- [ ] `sjtu services pending` / `services history` / `services search <keyword>`（只读先行）

### ⚪ S3e — 生活服务（一卡通余额 / 宿舍电费 / 校车）

- [ ] 调研 SP 接口（一卡通官方 SP、电费查询 SP）
- [ ] `sjtu card balance` / `sjtu elec balance --dorm <...>`
- [ ] 金额字段用 `rust_decimal::Decimal`，绝不用 f64

---

## ⚪ S4 — 一卡通消费明细（从原 S4 降级为 S3e 拓展或 Phase 2）

> 从 2026-04-23 起推迟：基础余额查询归 S3e，消费明细留到 Phase 2。

- [ ] `src/apps/card.rs` 消费明细爬取
- [ ] `src/commands/card.rs`：`card history`
- [ ] `tests/card.rs`

---

## ⚪ S5 — 教务 + Canvas（延后到 Phase 2）

> 2026-04-23 起推迟：用户当前优先级是让 Claude 操作"阅读/交互"类的水源/消息/日程/办事/生活服务，教务 HTML 爬取延后。

- [ ] `src/apps/jwc.rs` / `src/apps/canvas.rs`
- [ ] `src/commands/schedule.rs` / `grades.rs` / `canvas.rs`
- [ ] 教学周计算（开学日期放 `config.toml`）
- [ ] 通知去重：`(source, notification_id)` 复合键

---

## ⚪ S6 — 测试 + CI

预估 1 天。

- [ ] 补齐 auth / cookies / output 单测
- [ ] `tests/smoke.rs`（真实 API，所有用 `#[ignore]`）
- [ ] `.github/workflows/ci.yml`（stable × windows-latest / ubuntu-latest / macos-latest × `cargo fmt --check` + `cargo clippy -- -D warnings` + `cargo test`）
- [ ] `Cargo.toml` 配 `[lints.rust]` / `[lints.clippy]`

---

## ⚪ S7 — 文档 + 发布

预估 1 天。

- [ ] `README.md`（安装、使用、合规声明、GIF 演示）
- [ ] `SKILL.md`（AI Agent 使用指南）
- [ ] `SCHEMA.md`（输出字段契约）
- [ ] `CHANGELOG.md`
- [ ] `LICENSE`（MIT）
- [ ] GitHub Release：`cargo-dist` 或手搓 matrix build，附 Windows / Linux / macOS 预编译二进制
- [ ]（可选）发布到 crates.io：`cargo install sjtu-cli`

---

## 📋 Phase 2（MVP 发布后）

- [ ] 图书馆：`sjtu books` / `sjtu renew` / `sjtu reserve`
- [ ] 邮件：`sjtu mails --unread`
- [ ] 校车：`sjtu shuttle`
- [ ] SQLite 本地缓存（通知 / 邮件增量同步；用 `rusqlite` 或 `sqlx`）

---

## 📋 Phase 3（社区需求驱动）

- [ ] 场馆预约：`sjtu gym book`
- [ ] 流程审批：`sjtu flow pending`
- [ ] 网络账户：`sjtu net`

---

## 进度记录

| 日期 | 阶段 | 完成内容 | 遗留问题 |
|------|------|---------|---------|
| 2026-04-22 | 规划 | `SJTU-CLI规划.md`、`CLAUDE.md`、`tasks/todo.md`、`tasks/lessons.md` 初版（Python） | - |
| 2026-04-22 | 规划 v2 | 技术栈切到 Rust，同步规划 / CLAUDE / todo 三份文档 | 等本机装 rustup 后开 S0 |
| 2026-04-22 | S0 | 骨架完成：Cargo.toml + 7 个 src/*.rs + 配置样例；build / clippy / fmt 全绿；`sjtu hello` YAML/JSON/管道全链路验证通过 | Table→YAML 占位、Windows ACL、tracing、tests/ 均留到后续阶段 |
| 2026-04-22 | S1 代码 | 加 7 个 dep；新增 auth/{mod,qr_login,qr_render,browser_extract} + commands/{mod,auth_cmds}；改 cli/lib/main；build / clippy `-D warnings` / fmt / `cargo test`（2 passed）全绿；`sjtu --help` / `status` / `logout` 输出符合预期 | 真实 `sjtu login` 扫码链路待人工验证；终端 QR 在小尺寸时可能扫不动；rookie 兜底依赖本机浏览器已登过 JAccount |
| 2026-04-22 | S1 验收 | 实战修两个 bug：入口 URL 应为 my.sjtu（CAS 自动跳 JAccount QR 页），`tab.get_cookies()` 只看当前域 → 改用 CDP `Network.getAllCookies` 跨域抓；扫码成功抓到 7 条 SJTU cookie 含 `JAAuthCookie`；status 读取链路 OK | 终端 QR 实测扫不动（fallback 浏览器窗口）；status 展示因 HashMap-by-name 去重少列 2 条同名不同域 cookie（仅展示瑕疵） |
| 2026-04-22 | S2 | 加 reqwest/tokio/mockito 依赖；main 改 `#[tokio::main]`；cookies 加 sub_session 三件套（带路径注入防御）；拆 `src/auth/cas/{mod,client,tests}.rs` 3 文件（主文件控 200 行内）；手动跟 302 链 + 逐跳 set-cookie + jaccount 落点检查；加 hidden `test-cas` 调试命令；clippy/fmt/test 8 passed 全绿 | test-cas 首 19420ms → 命中缓存 6ms（3200× 加速）；落点 URL 为教务 `login_slogin.html` 需 S3 消化；rookie 兜底仍未人工验；tokio dev 特性与 prod union 到 30KB 膨胀 |
| 2026-04-22 | S1/S2 瑕疵修复 | 联网查 RFC 6265 §5.3 确认 cookie 唯一键是 (name, domain, path) 三元组（原 S1 留白里的 `(name, domain)` 方案不够严格）；`Cookie` struct 加 `path: Option<String>`（`#[serde(default)]` 向后兼容旧 session.json）；`Session::redacted()` 返回 `HashMap<String, String>` 用 `name@domain,path` 三元组 key；`cas::follow_redirect_chain` HashMap 键升级 `(String,String,String)` 三元组并填 `c.path()`；`qr_login` / `browser_extract` 的 cookie 构造顺带填 path；`cookies.rs` 拆成目录模块 `cookies/{mod, io, tests}.rs`（每文件均 <100 行）；新增 3 个单测（redacted 同名不同 path 不覆盖 + None 域路径不 panic + mockito 证 CAS 链同名不同 path 各占一行）；clippy/fmt/test 11 passed 全绿 | 真实 SJTU 子系统的具体 Set-Cookie path 分布未实测（S3 接入教务时顺便校验）；已存在的 session.json / sub_sessions/*.json 里 path 字段为 null，下次抓取自动回填 |
| 2026-04-23 | S3 路线图调整 | 用户指示 S3 改成"Claude 可操作 5 子系统"：S3a 水源 → S3b 消息 → S3c 日程 → S3d 办事 → S3e 生活服务；原教务 / 一卡通明细 / Canvas 整体推迟到 Phase 2 | 详细规划见 `tasks/plan-next.md` |
| 2026-04-23 | S3a 代码 | 写好 OAuth2 通道（`src/auth/oauth2/{mod,follow,tests}.rs`）+ 水源 Discourse 客户端（`src/apps/shuiyuan/{mod,api,http,models,render,throttle,tests}.rs`）+ 5 个只读命令（`latest` / `topic` / `inbox` / `search` / 隐藏 `login-probe`）+ clap 接入 | **真实 shuiyuan checkpoint 零次**，代码可编译 / 25 测试绿，但 `%APPDATA%/sjtu-cli/` 目录都还不存在 |
| 2026-04-23 | S3a 瑕疵修复 | `bare_client()` 加 `.no_proxy()` 解决本机 HTTP_PROXY/HTTPS_PROXY 劫持 mockito 127.0.0.1 请求的问题；6 个挂的跟链测试恢复绿（25/25 全绿）；lesson 已记 | 仍未跑真实 S3a checkpoint |
| 2026-04-23 | S3a 扫尾 | 写 2026-04-23 状态快照 `tasks/status-2026-04-23.md`；lessons 追加 mockito 代理继承教训；todo.md 同步 S3 路线图；拆分 `src/cli.rs`（232→<200）+ `src/commands/shuiyuan.rs`（218→<200）到 ≤200 硬限；写 `tasks/plan-next.md` 详规接下来子阶段 | 真实 S3a checkpoint 仍待用户扫码触发 |
| 2026-04-24 | S3a 写端点 + 收尾 | 实装 `shuiyuan reply` / `like` / `new-topic` 三写命令（强制 `--yes` 二次确认 + CSRF token）；补 `delete-topic` / `delete-post` 写端点（`DELETE /t/<id>.json` + `DELETE /posts/<id>.json` + `finish_empty` 支持空 body）；真机 CP-W 验证功能正确（真实 422/403 分支来自水源产品约束：有回复的话题禁删、首楼保留）；删隐藏 `sjtu test-cas`（S2 过渡命令）；33 单测全绿 | CP-1/CP-2/CP-3 真实 checkpoint 仍待用户触发 |
| 2026-04-24 | S3b 启动：水源 PM 只读 | 用 curl + 现有 OAuth2 cookie 真机侦察 `/topics/private-messages/{user}.json`（`target/pm_*.json`）确认 schema 复用 `TopicList`/`TopicSummary`；新增 `PmFilter` 枚举 + `Client::messages` 方法（内部先拉 `/session/current.json` 取 username 拼 URL）；新增 `cmd_messages` handler + `MessagesData` + clap `Messages/Message` 两个子命令（Message dispatch 层直转 `cmd_topic`）；补 2 单测（URL path_segment 映射 + PM 列表反序列化含 archetype=private_message）；fmt/clippy/35 tests 全绿 | CP-M1/M2 真实 checkpoint 待触发；pm-send 写端点待做；`tests.rs` 行数已到 330，`api.rs` 256、`cli/shuiyuan.rs` 264 均超 200 行硬限，下轮"清理一下"时建议拆 tests 为 read/write 两份、api 拆 read/write、clap 枚举单独成文件 |
| 2026-04-24 | S3c 调研 + MVP 实装 | chrome-devtools MCP 实抓 Canvas `oc.sjtu.edu.cn` 所有 XHR → `tasks/s3c-canvas-planner.md` 定契约（链路 / 端点 / CLI / Checkpoint 6 节 283 行）；回写 `tasks/plan-next.md §S3c` 整段；实装 `src/apps/canvas/*`（7 文件 + README，对齐 shuiyuan/jwbmessage 骨架）+ `src/commands/canvas/*`（3 文件）+ `src/cli/canvas.rs`；鉴权走 PAT 独立文件 `sub_sessions/canvas_token.txt`；新增 `SjtuCliError::{CanvasApi, CanvasTokenInvalid}`；cargo test 53/53 全绿（新增 6 canvas 单测）；CP-C1/C2/C3/C4 真机全过（本账号今日 0 DDL，14 天内 2 条） | 错误路径仍走 anyhow bin-layer 文本而非 Envelope（与 jwbmessage/shuiyuan 同口径，统一留 S6）；planner/items 未接 Link 分页（per_page=100 单页够用）；iCal 路线未实装（留给 Phase 2 聚合日程命令） |
| 2026-04-26 | S3b pm-send 真机 CP-PM1 + 字段名 fix | 真机调用揭露水源 PM 字段名魔改：标准 Discourse `target_usernames` 在水源会被路由到死路径返 422 "您必须选择一个有效的用户。"；正确字段名是 `target_recipients`（curl + python 复刻三组对照实验定位）；改 `apps/shuiyuan/api_write.rs::pm_send` body 字段名 + `tests_write.rs` mockito 断言同步；rebuild release；CP-PM1 自发自收 topic 469487 → sent 视图正确显示 → `delete-topic` 返 200 但 PM 实际未删（GET /t/<id> 仍 200 完整 + `X-Discourse-Route: topics/destroy` 也是 no-op）→ 改用 `PUT /t/<id>/archive-message.json` 真把 PM 从 sent 移走（returned: 0 验证）；2 个 mockito pm_send 单测全绿；lessons.md 追加"水源 PM 字段名 + 删除语义都魔改"教训 | `archive-pm` 端点 sjtu CLI 还没接，目前只能 curl 兜底；`delete-topic` 在 PM 上的 false-success 行为没改，理想做法是 handler 先 GET 看 archetype，PM 自动转走 archive — 留下一轮 |
| 2026-04-25 | S3a/S3b 真机 CP 验收 | 8/8 真机 checkpoint 全过：CP-1 login-probe → `authenticated:true` `from_cache:true` `elapsed_ms=6` `current_user.id=72509`；CP-2 latest --limit 3 → `returned=3`；CP-3 topic 468808 --post-limit 5 → `posts[0].post_number=1` `username=Narrenschiff`；CP-4 inbox --unread-only → `returned=6`；CP-5 search "jaccount" --in post → `posts_count=50`；CP-6 二次 login-probe → `elapsed_ms=6 < 100`；CP-M1 messages --filter inbox → `username=<水源昵称>` `returned=2`；CP-M2 message 404691 三 render 模式 (plain/markdown/raw) 语义全对（plain 剥 md / markdown==raw 保留）；**根因诊断**：本次卡 30+ 分钟全因 release binary 是 2026-04-23 16:55 编的旧版本（缺 `apps/shuiyuan/http.rs` 的 `pool_idle_timeout(0)` + `http1_only` 等修复）→ `cargo build --release` 重编后立刻通；本机网络须设 `HTTPS_PROXY=http://127.0.0.1:10808`（Clash mixed port），直连 DNS 解析水源超时；新增 `examples/proxy_diag.rs` 三组 builder 对照实验 `Default / Proxy::all / no_proxy + sjtu builder` 已删 | 写端点 CP-W4 (new-topic) 真机未触发；S3b pm-send 写端点未实装；S3b 交我办消息中心 SP 调研未做（待用户配合 chrome-devtools MCP）；S3d 办事大厅 / S3e 生活服务尚未启动 |
