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
- [x] **CP-W4 真机（2026-04-26）**：`sjtu shuiyuan new-topic "..." "..." --yes --yaml` → 200 返 `topic_id=469507 / post_id=8805252 / topic_slug=topic / cooked` 三件套，**上行 post 路径完成验证**。下行 `delete-topic 469507` → 422 "请与网站管理员联系"（水源 site-wide 禁用普通用户 self-delete top-level topic，与 04-24 reply→delete-post 成功路径不同）；`delete-post 8805252` → 403（首楼保留）；75s 后重试 delete-topic 仍 422 排除限流。Web UI 弹窗"您无权删除此话题，请提交举报让版主注意"——确认非 CLI bug。最终 web 手工 edit 标题→"加油喵～"/ 首楼 raw→"加油做最好的自己" 无害化收尾，bot fingerprint 消除。3 条 lessons 已记 (`tasks/lessons.md` 2026-04-26 第二条 entry)
- [x] **S3a Checkpoint 汇总**：CP-1..CP-6 全绿（2026-04-25），写操作 reply/like/delete-* 真机已 2026-04-24 验过；CP-W4（new-topic）2026-04-26 上行验证完成、下行受站点配置约束

### ⚪ S3b — 消息中心

- [x] 调研水源 PM 端点（Discourse `/topics/private-messages{-sent,-unread,-new}/{user}.json` + `/t/<id>.json` 复用 TopicDetail）— 2026-04-24 curl 真机侦察 `target/pm_*.json`
- [x] 代码实装：水源 PM 只读（`sjtu shuiyuan messages --filter <inbox|sent|unread|new>` + `sjtu shuiyuan message <id>` 复用 `cmd_topic`）— 2026-04-24 完成，35 单测绿
  - 新增 `apps::shuiyuan::PmFilter` 枚举 + `Client::messages(filter, page, limit) -> (username, TopicList)`（内部先拉 `/session/current.json` 取用户名）
  - clap：`shuiyuan::PmFilterArg` + `Messages {filter, page, limit}` / `Message {id, post_limit, render}`（后者 dispatch 层直转 `cmd_topic`）
  - 单测：`pm_filter_path_segments_are_correct`（4 个 URL 段映射）+ `parse_pm_inbox_topic_list`（带 `archetype=private_message` 的 TopicSummary 反序列化）
- [x] **CP-M1 真实 checkpoint**：`sjtu shuiyuan messages --filter inbox --yaml` → `username=<水源昵称>` / `returned=2` / 含 PM topic id+title — 2026-04-25 真机
- [x] **CP-M2 真实 checkpoint**：`sjtu shuiyuan message 404691 --post-limit 3 --render {plain|markdown|raw} --yaml` 三模式语义验证：plain 剥 markdown 标记（`[details=...]` → `details="..."`、链接降级）；markdown/raw 保留原始 markdown — 2026-04-25 真机
- [x] S3b 写端点：`sjtu shuiyuan pm-send <username> <title> <body>`（POST `/posts.json` + `archetype=private_message` + **`target_recipients=...`**，水源魔改字段名，非标准 Discourse 的 `target_usernames`），默认 `--yes` — 2026-04-26 真机 CP-PM1 通过：自发自收 topic 469487 → sent 视图返回 → archive 清理。代码 fix：`api_write.rs::pm_send` 字段名 + `tests_write.rs` mockito 断言同步
- [x] S3b 补端点：`sjtu shuiyuan archive-pm <topic_id>`（PUT `/t/<id>/archive-message.json`）+ `delete-topic` 加 `archetype=private_message` 预检（PM 路径直接拒并指向 archive-pm，避免 silent no-op 假成功）— 2026-04-26 真机 CP-PM2 + CP-DT-PM 双绿。代码：`api_write::archive_pm` + `Client::archive_pm` + `cmd_archive_pm` + `cmd_delete_topic` 加 `client.topic()` 预检 + `models::TopicDetail.archetype` + `cli::shuiyuan::ShuiyuanSub::ArchivePm` + 2 mockito 单测（55/55 全绿）
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

### ⚪ S3d — 办事大厅（2026-04-27 锁定方案，先做 S3d 再做 S3e）

> **协作模式**：半自动（仿 S3f i.sjtu SOP）。CLI 只读、决不点写按钮。
> **硬红线（重申，CLAUDE.md 已落）**：禁止 任何 提交 / 确认 / 保存 / 绑定 / 修改 / 删除 / 退订 / 申请 / 撤回 按钮；chrome-devtools 仅 take_snapshot / take_screenshot / list_network_requests / get_network_request / evaluate_script（read-only JS）；即使 session 在握也只当只读访客。
>
> **MVP 缩小起步**：只做"待办"列表（最高频、风险最低、纯只读）；历史 / 搜索 = phase-2。

**调研 → 实装 5 步（不许偏离顺序）**

- [x] **CP-D0 调研** ✅ 2026-04-27：抓获 `/api/task/me/processes/{todo,completed,cc}` 三端点 + envelope `{errno,error,total,entities}` + entity schema 差异（todo 嵌套 `process`，completed/cc 铺平）；待办端点 = `GET /api/task/me/processes/todo?thing=false`；纯 Cookie 鉴权（JSESSIONID + keepalive），需 `X-Requested-With: XMLHttpRequest`
- [x] **CP-D1 契约固化** ✅ 2026-04-27：契约写入 `tasks/isjtu_investigation.md` §6（**注意：§3-5 已被 i.sjtu SP 占用，办事大厅落 §6**），含 §6.1 鉴权 / §6.2 端点速查 / §6.3 todo / §6.4 completed / §6.5 cc / §6.6 已知坑
- [x] **CP-D2 用户确认** ✅ 2026-04-27：用户口头通过（"ok 严格准确安全地进行下一步"）
- [x] **CP-D3 实装** ✅ 2026-04-27：写 `apps/services/{mod,api,http,models,throttle,tests_parse}.rs`（6 文件，最长 161 行）+ `commands/services/{mod,handlers,data}.rs` + `cli/services.rs`，全部 < 200 行硬限；CAS 复用 `cas_login("services", "https://my.sjtu.edu.cn/ui/app/")`，**无需改 cookies 模块**（jwbmessage 已用同一 my.sjtu 入口跑通，sub-session 是按文件名隔离）；不复用 jwc SP HTTP helper（独立 http.rs 142 行）；`code=="ADD"` 在 handler 用 `partition` 拆 `my_applications` / `awaiting_my_action`；`--with-identity` 控 owner.name/owner.id 脱敏（默认 `***`）；新增 `Services` clap variant 注册到顶层；`cargo fmt` / `cargo clippy --all-targets -- -D warnings` / `cargo test --lib` (64 passed) 全绿；`sjtu services pending --help` 渲染 OK
- [x] **CP-D4 真机** ✅ 2026-04-27：`sjtu services pending --json` 走通（CAS 缓存命中，无浏览器弹窗）；`total/returned=1`，`code=="ADD"` 分流正确（my_applications=1, awaiting_my_action=0）；`--with-identity` 对称暴露/默认脱敏 owner.{name,id} 验证通过
- [x] **CP-D4.1 身份泄露补丁** ✅ 2026-04-27：CP-D4 输出里发现 `process.name="<工号> <真名> 研究生海外交流项目回校报到申请"`，服务端把工号 + 真名嵌进流程标题；落 D 方案——默认脱敏整字段置 `None`（`app.name` + `entry` 已足够定位流程），`--with-identity` 才出原值；`handlers.rs::redact_owner` → `redact_identity`；`cargo fmt` / `cargo clippy --all-targets -- -D warnings` / `cargo test --lib` (64 passed) / release 构建 / 真机双向验证全绿

**Phase-2（D-MVP 跑通后再启）**：`services history`（completed 端点 + 过滤参数 mine/status 现场补抓） / `services cc` / `services search <kw>`

### ⚪ S3e — 生活服务（一卡通余额 / 宿舍电费）

> **顺序**：S3d 完成 CP-D4 后再启动；不并行，避免上下文跳跃。
> **金额硬约束**：`rust_decimal::Decimal`，绝不用 f32 / f64；JSON 序列化为字符串。
> **同样的硬红线**：只读、不点充值 / 绑定 / 任何写按钮。

**调研 → 实装 4 步（不许偏离顺序）**

- [x] **CP-E0 调研** ✅ 2026-04-30：抓获 elec.sjtu.edu.cn 全 9 端点（`/api/me/info` 当前用户绑定房间 / `/api/ws/sydl` 余额（关键） / `/api/rechage/{ydl,ydlmx}` 月度+日明细 / `/api/comm/{xqdm,lddm,lcdm,roomdm,mdids}` 房间字典链）；envelope `{errno,error,total,entities}` 与办事大厅同款；纯 Cookie 鉴权（JSESSIONID + keepalive，独立子域 sub-session）；**金额混合类型**：`/api/ws/sydl` 字段是 string（"180.78"），`/api/rechage/{ydl,ydlmx}` 是 number（f64）—— CLI 端用 `serde_with::DisplayFromStr` 或 `deserialize_with` custom 把两类统一收成 `rust_decimal::Decimal`；**`/api/rechage/ydl` 的 `total:0` 是服务端误标**，entities 永远 1 条；硬红线 happy：全程只 GET，"立即支付"按钮未触
- [x] **CP-E0.1 一卡通调研结论** ✅ 2026-04-30：`my.sjtu.edu.cn/api/task/me/apps` 里 "我的校园卡" 用 `taskcenter://edu.sjtu.push/campusCard` 移动 deep-link，**web 无 SP**；`ecard.sjtu.edu.cn` 校园网内访问限制，off-campus 跳 `restrict.sjtu.edu.cn` —— **S3e MVP 仅做电费**，一卡通余额延 phase-2 待用户在校网时再现场抓
- [x] **CP-E1 契约固化** ✅ 2026-04-30：写入 `tasks/isjtu_investigation.md §7`（§4-5 已被 i.sjtu SP 占用，§6 已被办事大厅占用，生活服务落 §7），含 §7.0 范围实情 / §7.1 鉴权 / §7.2 端点速查 / §7.3-5 三个关键 API 响应 / §7.6 已知坑（金额混合类型 / 独立 sub-session / total 误标 / MVP 命令集）
- [x] **CP-E2 用户确认** ✅ 2026-04-30：契约 / Decimal 路径 / 一卡通 phase-2 / 命令名 `balance / usage / history` 三件套全部口头通过
- [x] **CP-E3 实装** ✅ 2026-04-30：写 `apps/elec/{mod,api,http,models,throttle,tests_parse}.rs`（6 文件，最长 144 行 / mod 29 行）+ `commands/elec/{mod,handlers,data}.rs`（3 文件）+ `cli/elec.rs`（46 行），全部 < 200 行硬限；`Cargo.toml` 加 `rust_decimal = { version = "1", features = ["serde"] }`（CLAUDE.md 已预批，金额硬约束）；CAS 入口 `cas_login("elec", "https://elec.sjtu.edu.cn/")` 独立 sub-session；金额混合类型用 `models::decimal_str_or_num` 自定义 ser/de 统一收成 `Decimal` + 输出字符串；Envelope 用 `bound(deserialize = ...)` 显式重写 serde bound 规避 `T: Default` 传染；新增 `Elec` clap variant + dispatch arm；`cargo fmt` / `clippy --all-targets -- -D warnings` / `test --lib` (71 passed，新增 6 个 elec 单测) / `build --release` 全绿；`sjtu elec --help` / `balance --help` / `usage --help` / `history --help` 渲染 OK
- [x] **CP-E4 真机** ✅ 2026-04-30：用户在校园网 Windows 11 cmd 跑通 `sjtu elec balance` / `usage` / `history --days 7` 三件套；逐项契约校验全过 —— `balance` D10-406 / `isbind:true` / 4 个金额字段全部字符串形式（`'284.25'` 不是 `284.25`，避开 f64 精度坑）；`usage` 当月 2026-04 / `last:80.55` / `now:62.44`（`total:0` 误标已忽略）；`history` 区间 `2026-04-24`~`2026-04-30` returned 6（服务端漏当天 04-30，与 §7.6 文档预期一致），`total_kwh:'18.88'` Decimal 累加精确（3.37+2.16+3.78+2.40+1.94+5.23 手算复核）；身份字段（name/work_no/account/dept）默认全部抹掉，输出只保留 `room` 位置标识 —— **S3e MVP 收尾**

**Phase-2**：一卡通余额（需校网）；一卡通消费明细（原 S4）；校车信息（如有 SP）

**S3e 留白**（不阻塞 S4，记账）：
- `isbind:false`（未绑定房间）case 真机未验，目前会返回 `entities 为空` 错误，未来可加 friendly 提示引导用户先在 elec.sjtu.edu.cn 绑定房间
- `--days > 30` 范围未真机验证（CLI 守 1..=90 上限，但服务端可能有自己的截断行为）
- 一卡通余额 / 消费明细仍是 phase-2，待用户在校网时现场抓 `ecard.sjtu.edu.cn` 或交我办 mobile deep-link 的 web 替代

### 🟡 S3f — i.sjtu 教务系统（jwc / ZF 正方）

> **2026-04-26 起激活**：S5"教务"原计划 Phase 2 延后，现因用户主导调研 i.sjtu.edu.cn 而前置。i.sjtu = ZFSOFT 正方教务（**不是**交我办聚合门户；交我办在 my.sjtu.edu.cn）。
>
> 详细规格 + 字段表 + 调研 SOP 见 `tasks/isjtu_investigation.md`；通用范式 + 红线 + 半自动协作模式见 `tasks/lessons.md` 2026-04-26 第一条 entry + CLAUDE.md "i.sjtu / 交我办 硬红线"。

**红线（CLAUDE.md 已落）**：信息维护 / 选课写菜单 / 教学评价 / 报名申请 / 任何 form submit 全禁；只做信息查询类只读 SP。

**半自动 chrome-devtools 调研（每个 SP 一发，由用户点查询按钮，CLI 抓 network）**

- [x] **N305005 学生成绩查询** — 单阶段 POST + 标准 envelope + 总评字段 `cj/bfzcj/jd/xfjd` — 2026-04-26 §2.1
- [x] **N2151 个人课表查询** — 自动加载 `cxXsgrkb`，专属 envelope（kbList + xqjmcMap），课表渲染 `xqj/jc/zcd` — §2.2
- [x] **N309131 GPA / 学积分查询** — **两阶段**（先 `tjGpapmtj` 算→返 `"统计成功！"`，再 `cxGpaxjfcxIndex` 拉），item 含 GPA/排名/学积分/通过率 — §2.3
- [x] **N358105 考试信息查询** — 单阶段，item 含 `kssj` 复合时间 + `cdmc` 考场 + `jsxx` 监考 — §2.4
- [x] **N305007 学生成绩明细查询** — master-detail（`cxXsKcList` + `cxXsKccjList`，jxb_id join），detail 含 `xmblmc` 项目 + `xmcj` 项目分 — §2.5
- [x] **N551225 学生修业情况查询** — 1+N 调用（`xsxyqk_ckXsXyxxHtmlView` overview 三级模块 + `xsxyqk_ckDynamicGridData` 各模块课程，`xfyqjd_id` join），`xh_id` 在 URL — §2.6
- [x] **N2154 学生课表查询（按周次）** — `xskbcxMobile_cxXsKb`，`zs=<周次>` + `rqazcList[]` 当周日期映射；`oldzc` = **16-bit 周次掩码**、`oldjc` = 节次掩码（CLI 用 mask 比 parse 字符串干净）— §2.7
- [x] **N153521 培养计划课程查询（含 N153540）** — 默认全校 412 条，CLI **必填 `zyh_id`** 过滤本专业；`xsxxxx`/`xsdm_0X` 含动态字段 — §2.8
- [x] **N532560 毕业设计成绩查看** — 端点 + envelope 已验，items 空（用户未到毕设阶段，正常） — §2.9

**实装（调研收口后开工）**

- [x] `src/apps/jwc/{mod,api,bind,http,models,throttle,tests_parse}.rs` — ZF 通用 `JwcPage<T>` envelope + post_form_json + visit_sp_page（pre-GET 绑模块）+ /jaccountlogin CAS 入口 — 2026-04-26
- [x] `src/commands/jwc/{mod,data,handlers}.rs` — MVP 仅 grades — 2026-04-26
- [x] `src/cli/jwc.rs` — `sjtu jwc grades [--xnm] [--xqm] [--page] [--limit]` — 2026-04-26
- [x] **CP-J1 真机**：`sjtu jwc grades --xnm 1900 --xqm 3 --json` 跑通 N305005（用不存在学年触发空 items 安全验全链路），CAS 8 跳 + SP 页面预 GET + POST 全过 — 2026-04-26
- [ ] **CP-J1.b**：用户私下跑真学年（`--xnm 2025` 等）确认实数据形态 + 解析 OK（CLI 默认输出脱敏，避开学号/姓名进我上下文）
- [ ] N2151 课表 / N309131 GPA / N358105 考试 各 SP 实装 + CP-J2..CP-J4
- [ ] CP-J5..CP-Jn 按 §2.5..§2.9 5 个 phase-2 SP 逐个 checkpoint
- [ ] `tests/jwc_*.rs` mockito 端单测（不打真服务器）
- [x] **T1 jwc 课表衍生命令 today / week / next + --grid** ✅ 2026-05-12（14 task plan + 9 commit）：N2154 周次端点 + oldzc/oldjc bitmask 过滤 + period_clock 1-13 节时刻 join + 反推当前周 + 24h cache + comfy-table grid 渲染（day + week）
  - **新模块**：`src/apps/jwc/api/{schedule.rs::schedule_by_week+infer_current_week, week_cache.rs, term.rs}` / `src/apps/jwc/period_clock.rs`（DEFAULT_TABLE + jc_positions + is_in_week）/ `src/commands/jwc/{schedule_handlers.rs, schedule_helpers.rs, schedule_next.rs}` / `src/cli/jwc/{mod.rs+schedule_cli.rs}` / `src/output_grid.rs`（comfy-table day + week 渲染）
  - **改 envelope**：`KbItem` 加 `Option<oldzc/oldjc>` + `Schedule` 加 `rqazc_list: Vec<RqAzc>` + 新 `TodayData/WeekData/NextData/TodayItem/NextItem`（全 additive）
  - **新依赖**：`comfy-table = "~7.1"`（锁 7.1.x MSRV 1.64，避 7.2 MSRV 1.85 超 rust-version 1.75）
  - **真机 8 步全过**（T12 SJTU 校园网 2026-05-12）：today 显当周二剩余课 ✓ / week 显 5/11-5/17 7 列 / week --zs 1 显 03-02..03-08 / next --within 7 跨周排序 / next --within 31 12.4s / cache __current__:11 ✓ / cached 7.4s（first 12s 节省 4.6s）/ --grid comfy-table 7×8 行肉眼可读
  - **T12 暴露 + 已修**：ZF 9 SP 不再接受空 xnm/xqm（`default_xnm_xqm_by_date` 按月推 fallback）；plan stale 类型（oldzc/oldjc string / rqazc.xqj number）已在 dispatch prompt 校正；详见 `tasks/lessons.md` 2026-05-12 第二段
  - **follow-up bug**（独立 task）：`sjtu logout` 只清主 session 不清 sub_sessions/<name>.json，cas_login 命中 stale cache → 修 `cmd_logout` 清整个 sub_sessions/ 目录
- [x] **T2 jwc GPA 计算 + 学期均分** ✅ 2026-05-13（9 task plan + 8 commit）：N309131 两阶段 SP（step1 统计触发 + step2 拉结果）+ 排名双轨解析（`gpapm_parsed`/`xjfpm_parsed: Option<RankPair>` 附加字段，camelCase 序列化）+ `gpa-by-semester` 多学期循环（4 年 × 3 学期，fail-soft，exit 始终 0）
  - **新模块**：`src/apps/jwc/models/gpa.rs`（Gpa / RankPair / parse_rank / fill_parsed）/ `src/commands/jwc/data/gpa.rs`（GpaData / GpaBySemesterData / SemesterGpa / SemesterFailure / SemesterKey）/ `src/commands/jwc/gpa_handlers.rs`（cmd_gpa / cmd_gpa_by_semester）/ `src/apps/jwc/tests_parse.rs`（新增 N309131 fixture round-trip + fill_parsed 集成测试）
  - **改文件**：`commands/jwc/data/mod.rs`（拆出 gpa.rs 后的 re-export 层）/ `commands/jwc/mod.rs` / `cli/jwc/mod.rs`（GpaBySemester variant + dispatch arm）
  - **真机 smoke matrix**（6 单学期 + gpa-by-semester 12 学期循环 + fail-soft 边界 2030）：hxkc/njzy gpa+rank ✓ / hxkc/nj server 返 HTML 错误页 → cmd_gpa 报错 ✗（by design）/ qbkc/njzy gpa+xjf ✓ / gpa-by-semester 12 学期 56.5s（server-side N309131 step1 4-5s/次）succeeded=N failed=12-N exit=0 ✓ / fail-soft 边界 xnm=2030 all-failed exit=0 ✓
  - **camelCase 输出**：`gpapmParsed` / `xjfpmParsed`（`#[serde(rename_all = "camelCase")]` on Gpa）
  - **data.rs 200 行触底拆分**：`commands/jwc/data.rs` → `data/{mod.rs, gpa.rs}` 两文件
- [x] **T5 jwc 校历 iCal 导出 MVP** ✅ 2026-05-15：T1-T9 全部完成
  - [x] T5 Plan T6：`cmd_calendar` handler + envelope + fail-soft（commit `980859f`）
  - [x] T5 Plan T7：CLI Calendar variant + dispatch（commit `05642c3`）
  - [x] T5 Plan T8：README / SKILL / todo / lessons / CLAUDE 文档收尾（commit `f6ef916` + `c630284` 文档与代码对齐修正双轨）
  - [x] T5 Plan T9：用户亲跑 Google / Apple / Outlook / 手机本地 4 端日历 import + 重复 import 幂等 smoke ✅ 2026-05-15（用户报告"全部都成功了"）
  - [x] T5 整体完成：jwc 校历 iCal 导出 MVP ✅ 2026-05-15
  - **T9 真机新发现**（详见 `tasks/lessons.md` 2026-05-15 段）：
    - DTSTAMP 跨次刷新让 `hashHex` 不能跨次对比 → SKILL.md L141 已修，明确幂等靠 UID
    - README / SKILL 文档与 handler 实际逻辑不一致 3 处（`--to` envelope 触发条件）→ commit `f6ef916` 修
    - iOS 微信 `.ics` 分享菜单不带"日历"（微信内部分享 ≠ iOS 系统 share sheet），走"存储到文件 → 文件 App 打开"或邮件附件即可
    - **staleness-fix 覆盖盲区**：jwc sub_session 客户端 captured_at fresh 但 ZF 服务端 session timeout（30 分钟），第一次跑 calendar eventCount=0 + warnings 含 redirect → login_slogin.html。临时修复=精准删 `sub_sessions/jwc.json` 触发主 session CAS 自动跳转刷新
    - [x] **根治 follow-up（2026-05-16 完成）**：CAS retry 层落地，jwc 9 个 call site 透明 auto-refresh
      - spec：`docs/superpowers/specs/2026-05-15-cas-retry-layer-design.md`
      - plan：`docs/superpowers/plans/2026-05-15-cas-retry-layer.md`（8 tasks TDD）
      - 通用 helper：`src/auth/cas/retry.rs::with_cas_refresh`（51 行同构 canvas_video::with_token_refresh + `with_refresh_inner` 注入 refresh fn 单测友好）
      - 强类型信号：`SjtuCliError::SubSessionStale(&'static str)` variant，downcast 跨 anyhow 链（3 inline tests + 3 cross-module sanity tests `tests/cas_retry_signal.rs`）
      - jwc 接入：9 个 call site（grades / schedule / gpa / gpa-by-semester / exams / today / week / next / calendar）+ `ical/handler.rs::fetch_all` 改 Result 后 stale-check 重 raise（修 fail-soft 吃信号 bug）
      - T3 spec 漏洞补丁：commit `4d7f52d` fix `bind.rs::visit_sp_page` 同域 redirect 也抛 SubSessionStale（T8 真机暴露）
      - T8 真机 CP-CR-1/2/3 全过（详 `tasks/lessons.md` 2026-05-16 段）
      - **未接入子系统**（spec NG1 明示）：elec / services / jwbmessage 暂不动；各自 SP 的 stale 信号需独立调研后再接入

### 🟡 S3g — Canvas 课堂视频 (v.sjtu / "课堂视频new") — 2026-05-07 启动

> **范围**：批量下载用户已注册课程的课堂录播视频 (mp4) + 可选音频提取。**完全独立于 S3c Canvas PAT 路径**：本路径走 LTI 1.3 OIDC + v.sjtu 后端，不走 PAT。详细契约见 `tasks/canvas_video_investigation.md`。

> **红线**：CLAUDE.md "i.sjtu / 交我办" 硬红线**不适用**本系统（v.sjtu 无写按钮 / 无选课信息维护类）；但仍按"只读访客"原则——CLI 实装只调查询端点，**不调埋点 (`burialPoint`) / 不写"上次观看时间" / 不上传笔记**。

> **PII 策略（用户 2026-05-07 确认）**：默认抹学生端 PII (`userCode` / `lastWatchTime` / `playCount` / `playTime` / `playTimes` / `accessToken.jwt_token`)；教师姓名作为公开教学信息默认留；`--with-identity` 才全出。

> **实装路线（用户 2026-05-07 确认）**：CP-V1 LTI launch 走 **headless_chrome**（复用 S1 依赖），不手刻 OIDC implicit flow。

**调研 → 实装 5 步（不许偏离顺序）**

- [x] **CP-V0 调研** ✅ 2026-05-07：chrome-devtools MCP 半自动抓 LTI 1.3 OIDC 三跳 + `getAccessTokenByTokenId` (含 token + courId + ltiCourseId) + `findVodVideoList` (16 讲列表) + `getVodVideoInfos` (双机位 mp4 直链 + 时效签名)；契约写入 `tasks/canvas_video_investigation.md` 10 节；3 个关键 gotcha 已记（canvasCourseId 必须用加密 courId 不是数字 ID / token 用 data.token 不是 accessToken.jwt_token / mp4 URL 含 unix 秒签名不可缓存）
- [x] **CP-V0.1 用户确认** ✅ 2026-05-07：契约 + LTI launch 路线 + PII 策略全部口头通过
- [x] **CP-V1 实装 LTI launch + token 提取** ✅ 2026-05-08：subagent worktree 跑完，主仓库手工合并通过四关验证。落地 13 文件：
  - `apps/canvas_video/{mod,models,auth,auth_chrome,http,api,throttle,tests_parse}.rs`（auth.rs 拆出 auth_chrome.rs 守 200 行硬限；同步 chrome 调用包在 `tokio::task::spawn_blocking`）
  - `cli/canvas_video.rs`（`CanvasVideoSub::List` enum + dispatch；与 PAT `Canvas` 独立顶层 variant，命令是 `sjtu canvas-video list <id>`）
  - `commands/canvas_video/{mod,handlers,data.rs}`（`cmd_list` 走 load_session → connect → list → 过滤 vide_audit_status==3 → 按 course_begin_time 排序加 1-based seq）
  - **不持久化 token / cour_id**（TTL 1-3 小时，每次重跑 LTI launch ~3-10s）；只复用主 `session.json` 的 jaccount cookie 注入到 chrome
  - PII 红线落实：`models::TokenData` 故意只反序列化 4 个字段（token / params.{courId, ltiCourseId, courseName}），`userCode/userName/jwt_token` 等 PII **不入 struct**，编译产物里也不带
  - 验证：`cargo check` / `clippy -D warnings` / `test --lib`（82 tests，含 7 个新 mockito 单测）/ `fmt --check` 全绿；`sjtu canvas-video list --help` clap 渲染正常
  - 实装与原计划差异：①sub-session 落盘部分回滚（token 不持久化，但 oc 域 cookie 通过 cas_login 落 `canvas_oc.json`）②`get_video_info` / `findVodVideoList`-by-`ltiCourseId` 留给 CP-V3，CP-V1 只到 `list_lectures`
  - **2026-05-08 真机首跑暴露 4 处补丁**（已修，单测仍 7/7 PASS）：①`auth.rs` 起手插 `cas_login("canvas_oc", external_tools URL)` 给 oc.sjtu 签认证 session（缺这步 chrome navigate 被踢 `/login/canvas` 卡死）②同步移除 `lti_launch` / `Client::connect` 的 dead `main_session` 参数（cas_login 内部 `load_session`）③`auth_chrome::wait_for_landed` 超时错误附最后 URL（脱敏到 host+path）④`build_cookie_params` 对 cas 落盘 cookie 的空 domain 回填 `oc.sjtu.edu.cn`（cas 模块 RFC 6265 §5.3 全局 bug，待 S2 修通后此兜底可删）
- [x] **CP-V2 list 真机** ✅ 2026-05-08：调研 + 一行修复 + 真机验证全过。`sjtu canvas-video list 88168 --json` 返 18 讲（CP-V0 旧记录 16 → 今日新增 2 讲：2026-05-08 当日两节），envelope schema_version="1"，PII 字段（cour_id / lti_course_id）按 `with_identity=false` 正确截短到 12-char prefix。
  - **修复一行**：`src/apps/canvas_video/auth.rs:48` 把 `cas_target` 从 LTI launch URL 改为 `https://oc.sjtu.edu.cn/login/openid_connect`（即 `/login/canvas` 静态页里 jAccount 按钮的 `<a href>`）。先前误判"SSO 触发要点击按钮"，实测按钮就是普通 `<a>` 超链接，纯 GET 触发 OIDC 302 链，cas_login 既有逻辑直接能跟。
  - **调研方法**：chrome-devtools MCP 协议 `Network.enable` 持续超时 → 改纯 `curl` 抓 `/login/canvas` HTML 静态页 + grep 出 `<a href="/login/openid_connect">` → 单跳 `curl --max-redirs 0 /login/openid_connect` 验证 302 → `jaccount.sjtu.edu.cn/oauth2/authorize?client_id=lACSIkmjF7lRHNKaVrIp&...`（OAuth 2.0 / OIDC Authorization Code Flow，含 JWT state）。**完全只读**，零状态变更风险。
  - **性能**：cas 首跑 10528ms（OIDC 三跳 + cookie collect）→ 缓存命中 7ms（**1500× 加速**）。Chrome LTI launch 仍需 ~30s（启动 Chrome + navigate + 等 v.sjtu 落地），CP-V3 下载阶段不再触发 launch（Bootstrap 在内存复用）。
- [x] **CP-V3 单讲下载** ✅ 2026-05-08：`sjtu canvas-video download 88168 --lecture 1 --to ./tmp/v3 --json` 单讲 800MB 完整 mp4 落盘，envelope `ok=true`，PII（video_id / mp4_url）按默认正确脱敏。
  - **实装**：`apps/canvas_video/download.rs`（187 行，Range 分片并发 + 0/3s/10s/25s 梯度 backoff + part 合并原子 rename）+ `apps/canvas_video/api.rs::get_video_info`（POST `getVodVideoInfos` urlencoded）+ `apps/canvas_video/api_form.rs`（form POST helper，与 post_json 同构）+ `apps/canvas_video/models_video.rs`（VideoInfoData 仅 4 字段，PII 不入 struct）+ `commands/canvas_video/download_handler.rs`（cmd_download + safe_filename 处理 Windows 禁字符）+ `cli/canvas_video.rs::Download` 子命令。**所有文件 ≤ 200 行**（最大 download.rs 187 / download_handler.rs 127）。
  - **依赖增量**：`Cargo.toml` tokio features 加 `fs` + `io-util`（异步 read/write/rename/mkdir 必需，runtime feature 补全非新 crate）。
  - **CDN 504 教训**：默认 8 段并发对 SJTU 教学 CDN 过载（多段同时返 504 Gateway Timeout，3 次 retry 全军覆没）。降默认到 **4 段** + retry backoff 改梯度 `[0, 3000, 10000, 25000]ms` 才稳。真机第二次跑：段 0/1/2 直接成功，段 3（最末段）触发 504，靠 25s backoff 后 attempt 2 写满 210MB 救活。
  - **真机指标**：probe size=840,104,214B（800MB） / partial=true（CDN 支持 Range）；总 800,483ms ≈ 13min20s（LTI launch 30s + cas 缓存命中 / 段 504 backoff 链 ~5min / 真正下载 ~7min）；段 0/1/2 各 210MB 一次过，段 3 attempt 2 救活；part0-3 自动合并 + 原子 rename + 删 part，目录无残骸。
  - **PII 脱敏**：`video_id_redacted="2WsP4p8BsYbr***"`（前 12 + ***），`mp4_url_redacted="https://live.sjtu.edu.cn/...***"`（仅保 scheme+host），文件名"日语语言学专题研讨（2）(第1讲)" 中文括号合法保留 / 西文括号合法保留。
- [x] **CP-V3.1 加速：每段独立 TCP** ✅ 2026-05-08：根因 reqwest 默认 ALPN 协商 H2，N 段在底层多路复用一条 TCP 被 SJTU CDN 按 per-conn 限速 ~1MB/s（reqwest #976 + uv #17204 同坑）。`Client::builder().http1_only().pool_max_idle_per_host(0).tcp_nodelay(true)` 强制每段独立 TCP，对照 prcwcy/sjtu-canvas-video-download 的 aria2 -x 16 实证路径。CLI `--concurrency` 默认 4 → 8。段 i 内部 sleep i*50ms 错峰防 burst 触发 CDN 限流。**真机指标**：lecture 1 同条件重测 → 800,483ms → 201,946ms（**3.97× 提速**，1.05 → 4.16 MB/s）。byte-identical 对比 V3 产物（840,104,214B）。download.rs 200 行整（CLAUDE.md 红线），无新依赖。
  - **HLS 路线证伪**：调研文档显示 `playTypeHls=true` 是误导参数名，响应只返 `rtmpUrlHdv`（mp4 直链），无 m3u8 字段；方案 3（HLS 多段并发 + ffmpeg 拼接）路径不可行，V5 不走该方向。
- [x] **CP-V3.2 双机位下载 `--all-channels`** ✅ 2026-05-09：用户实测 V3.1 ch0 只有教师机位，看不到 PPT。CLI 加 `--all-channels`（与 `--channel` clap `conflicts_with` 互斥），handler 走 `cmd_download_all` 顺序跑两路（channel 0 → 1），envelope 改 `channels: [ChannelOutput; 2]` + 共享 `course_id / lecture / video_name / video_id_redacted / duration_secs` + `total_bytes / total_elapsed_ms`。重构：抽 `resolve_target` + `download_one_channel` helpers，三个 utils（`safe_filename` / `absolutize` / `redact_url`）下沉到 `handlers.rs::pub(super)`。**真机**：lecture 1 双机位 → 1.18GB / 280,360ms（ch0 800MB/143s = 5.87 MB/s，ch1 411MB/91s = 4.73 MB/s，含 LTI launch ~30s）。文件 `_ch0.mp4` + `_ch1.mp4` 落盘 `tmp/v3-all/`。`download_handler.rs` 171 行 / `handlers.rs` 130 行 / `data.rs` 107 行 全部 ≤200 红线，零新依赖。
- [x] **CP-V4 批量 + 音频提取** ✅ 2026-05-09：`--lectures <SPEC>` 选择器（`all` / `1-5` / `1,3,5-7`）+ `--audio-only` ffmpeg 抽 m4a + `--keep-mp4` 保留 mp4。fail-soft（单讲单路失败进 `errors[]` 不阻塞）+ 断点续传（dest 已存且 size>0 → status=skipped）+ **临用临取**（每讲下载前才调 get_video_info 防 mp4 URL `key=` 1-3h 过期）+ stderr 进度行（不污染 stdout envelope）。
  - **新增文件**：`apps/canvas_video/ffmpeg.rs`（76 行，`ensure_ffmpeg` + `extract_audio` + `spawn_blocking`，零新依赖，std::process::Command）；`commands/canvas_video/lectures_spec.rs`（107 行，含 4 单测）；`commands/canvas_video/batch_handler.rs`（190 行，BatchArgs + cmd_download_batch + check_skip + derive_status）。
  - **改动文件**：`cli/canvas_video.rs` 166 行（加 `--lectures/--audio-only/--keep-mp4` 三 flag + dispatch 路由 batch / single / all-channels 三分支）；`commands/canvas_video/{data.rs,download_handler.rs,handlers.rs,mod.rs}`（拆 `resolve_target` / `list_audited_sorted` 到 handlers.rs 共用，DownloadData/ChannelOutput 加 `audio_path/mp4_kept` 字段）。
  - **真机指标**：① audio-only 单讲 lecture 1 ch0：105,243ms（mp4 下载 84s + ffmpeg 抽流 21s），840MB mp4 → 20MB AAC m4a（40× 压缩比，ffprobe 验过 codec=aac/codec_type=audio）；② 断点续传 lecture 1：21,133ms（仅 LTI launch，零下载零抽流），status=skipped；③ clap 互斥：`--lecture` × `--lectures` 触发硬错。
  - **行数**：所有 .rs 守 ≤200 行限（最大 batch_handler 190 / handlers 166 / data 158）。零新依赖。
- [x] **CP-V5.A LTI Bootstrap 缓存** ✅ 2026-05-10：`Client::connect()` 走 `cache::lti_launch_cached`，命中 sub_session 文件直接返 Bootstrap，未命中跑完整 LTI launch + save。30min 固定 TTL + 业务失败回退（`with_token_refresh` 在 cmd_list / cmd_download / cmd_download_all 套，token 401/403/业务码非 0 → 自动 `cache::clear` + 重 launch 一次；`cmd_download_batch` 不套，依赖 V4 fail-soft + 复跑幂等）。新 CLI：`sjtu canvas-video clear-cache [--course <id>] [--all]`。
  - **新增文件**：`apps/canvas_video/cache.rs`（200 行；CachedBootstrap struct + save_to_path/load_from_path/chmod_600/save/load/lti_launch_cached/clear；3 单测）；`apps/canvas_video/tests_cache.rs`（45 行，cache_expired_returns_none + cache_corrupted_returns_none）；`commands/canvas_video/retry.rs`（43 行，`with_token_refresh` + `looks_like_token_invalid`，从 handlers.rs 抽出来防超 200 行）；`commands/canvas_video/download_shared.rs`（70 行，`download_one_channel` + `prep` + `DOWNLOAD_REFERER` 共享 helper，从 download_handler.rs 抽出来防超 200 行）。
  - **改动文件**：`apps/canvas_video/{mod.rs,api.rs,tests_parse.rs}`（cache mod 注册 + `Client::connect` 一行换 + cache_round_trip 单测）；`commands/canvas_video/{data.rs,handlers.rs,download_handler.rs,batch_handler.rs,mod.rs}`（cmd_clear_cache + ClearCacheData + cmd_list/cmd_download/cmd_download_all 套 retry + import 切到 download_shared）；`cli/canvas_video.rs`（ClearCache variant + dispatch arm，clap `--course/--all` 互斥）；`cookies/{io.rs,mod.rs}`（`sub_session_path` 从 pub(super) 升 pub 给 cache.rs 用）。
  - **真机 4 关 (2026-05-10 12:38-12:40)**：① 关 1 冷启首跑（含全新 CAS handshake + Chrome LTI launch）40.3s — 18 讲全列出；② 关 2 缓存命中 1.98s，比关 1 省 ~38s（findVodVideoList HTTP 本身吃 ~1s 是底）；③ 关 3 `clear-cache --all` 后再跑 31.1s（canvas_oc warm，纯 Chrome LTI launch）— `cleared_count=1` 确认有缓存被删；④ 关 4 cargo fmt + clippy --all-targets -D warnings + test --lib 90 全绿。
  - **行数**：cache.rs 200 / handlers.rs 188 / download_handler.rs 156（拆出 download_shared.rs 后）/ batch_handler.rs 190 / cli 149；全部 ≤ 200 红线，零新依赖。
  - **过程踩坑**：①plan 估 cache.rs ~100 行，实装 ~200 行（save_to_path + chmod_600 + clear 三 helper 加起来比想象大）；②handlers.rs 在 T8 直接加 with_token_refresh 卡 200 行限，T8a 拆 retry.rs 释放；③T12 后 download_handler.rs `wc -l` 215 但 PowerShell `Measure-Object -Line` 漏报 194（CRLF 处理差异），T13a 拆 download_shared.rs 真实降到 156；④canvas_oc.json 服务端过期但本地 30 天 TTL 不识别 → cas_login 用陈旧 cookie 登 Chrome 被踢 /login/canvas → V5.A 编码无关的预存边界，删 canvas_oc.json 强制 fresh CAS 即可（与 V5.A 缓存完全独立）。
- [ ] mockito 端单测（不打真服务器）—— 已含 11 个（含 V3 `parse_get_vod_video_infos_minimal` + V4 lectures_spec × 4 + V5.A cache_round_trip / cache_expired_returns_none / cache_corrupted_returns_none），`download::*` 的 Range 分片单测延后到下一轮
- [x] **CP-V5.D audio-only Range 直下 + reqwest timeout 收紧** ✅ 2026-05-11：parse mp4 moov → Range-fetch audio chunks → 本地 mux m4a，跳过 mp4 落盘 + 跳过 ffmpeg。reqwest timeout 30 min → 90 s 段级 + 30 s inter-byte（V5.B L9 卡 13 min 的直接动机）。
  - **新增模块**：`apps/canvas_video/mp4_box/`（parser/trak/stbl/boxes 四文件 + tests，113-131 行/文件，含 8 unit tests）；`apps/canvas_video/m4a_mux/`（build_moov 重建最小 mp4 容器 + write_m4a_async spawn_blocking + 1 round-trip test）；`apps/canvas_video/audio_dl/`（locate + client + fetch + orchestrator + ranges + test_helpers 六文件，含 13 unit tests 用 mockito 启 127.0.0.1 server）。
  - **新增 envelope 字段**：`download_kind` (mp4-full / m4a-direct / skipped) + `bytes_downloaded` 在 `ChannelOutput` / `DownloadData` / `BatchData`，老字段语义不动（additive evolution per Confluent/Conduktor/yt-dlp consensus）。
  - **新增 env switch**：`SJTU_NO_FALLBACK=1` 调研期切换，V5.D 失败时 bypass fail-soft 直接 bail；默认仍 fail-soft 走旧 mp4-full + ffmpeg 路径。
  - **真机 L10 single-lecture smoke 验证**：`download_kind=m4a-direct`, m4a 22 MB（vs baseline 19.5 MB 同长度讲），elapsed 6.5 min（vs baseline 20 min sustained = 3.2× 加速），bytes_downloaded 676 MB（vs mp4-full 916 MB，节省 24%）。详 `tmp/v5d_phase2/_comparison.md`。
  - **关键 debugging 经验**：mp4 真实布局 `[ftyp 24][mdat 914 MB][moov 2.1 MB]` 不是 spec 假设的 head 1MB / tail 16MB 命中；初版 `locate_moov` 尾部翻倍探测从 `total - probe` 倒退总落在 mdat 中间 → 找不到 moov；修复后用 head 推算 mdat 后段 fetch。诊断 hook（`dump_top_boxes` + `hex_prefix`）内嵌生产代码，CDN 行为黑盒时 `RUST_LOG=info` 一开即见。详 `tasks/lessons.md` 2026-05-11 段。
  - **行数**：locate.rs 247（超 200 含必要诊断工具，已注释说明）/ orchestrator.rs 68 / m4a_mux/mod.rs 139 / build_moov.rs 182 / 其他 < 200。114 unit tests pass / 0 fail / 1 ignored。
- [ ] **V5.E（延后）chunk-level Range + Phase 2 完整 9 讲 batch**：V5.D 当前 sample-level Range 合并把 audio 间 video frame 也下了（705 MB vs 期望 22 MB，浪费比 32×）。改用 mp4 stco/stsc 表 chunk-level offset + size 整体下，理论可达 spec 38× 网络节省。配套：CDN 限流时 adaptive concurrency 降级 + HTTP/2 keep-alive 多路复用。Phase 2 9 讲 batch（baseline 对比）等 V5.E 优化后再跑（当前 V5.D L11 实测 retry overhead 让单讲 elapsed 抖动到 15 min+，9 讲 batch 估 2-3 h）。

**留白**：
- ~~双机位下载：MVP 默认下 `cdviChannelNum=0` 老师视角；`--all-channels` 双机位都下留 phase-2~~ ✅ V3.2 已实装（顺序跑两路 + envelope 含 `channels` array）
- ~~批量下载 + audio-only~~ ✅ V4 已实装（`--lectures all/1-5,8` + `--audio-only` + `--keep-mp4` + fail-soft + skip + 临用临取）
- ~~LTI launch 缓存到 sub_session 文件~~ ✅ V5.A 已实装（30min TTL + 业务失败回退 + clear-cache CLI；缓存命中 ~38s 提速）
- token 时效未现场抓多次确认；V5.A 30min 保守 TTL 远低于 token 真实 1-3h，命中率充足
- 字幕提取：`videSrtUrl` 实测 null + 用户原话"有出入"；V5.C 调研推荐 faster-whisper + large-v3 自带语音转录走 `transcribe` 子命令（V6 实装）
- V5.B 18 讲全量 audio-only smoke + V5.C 转录调研：留 V5.A 收口后单独触发

---

## ✅ S3 Phase 2 — 一卡通 OAuth2（已完成代码 + 单测；真机 CP 阻塞 client_id 2026-05-17）

> **plan**：`docs/superpowers/plans/2026-05-17-t4-ecard-oauth2.md`（T0-T17，17 任务，Subagent-Driven）
> **背景研究**：`docs/superpowers/research/2026-05-17-t4-update.md`（路径 C：OAuth2 + api.sjtu.edu.cn）

### 代码 + 单测部分（T0-T13）— 全部完成 2026-05-17

- [x] **T0** Cargo.toml +tokio net feature（commit 3f8c8bb）
- [x] **T1** util/decimal.rs 提取 + elec 切 import（commit 49153cf；5 visitor tests + 7 elec import 切换）
- [x] **T2** error.rs +CardOAuth/SecretMissing/Timeout 3 variants（commit a3a4946；3 tests）
- [x] **T3** oauth2_dev/secret.rs + chmod 600 守护（commit ed47bff；2 tests）
- [x] **T4** oauth2_dev/token.rs exchange_code + refresh（commit c5a9219；4 mockito tests）
- [x] **T5** oauth2_dev/callback.rs 本地 TCP listener + percent-decode（commit c11b845；6 tests）
- [x] **T6** oauth2_dev/authorize.rs URL 构造 + state + Browser open（commit fb35159；3 tests）
- [x] **T7** oauth2_dev/refresh.rs `with_token_refresh<F,Fut,R,RFut,T>`（commit 5a69697；7 tests）
- [x] **T8** oauth2_dev/mod.rs 顶层 CardOAuthSession + load/save/is_token_stale/refresh_and_save（commit 0675e01；5 tests）
- [x] **T9** apps/card/models.rs CardInfo + Transaction + dateTimAccount 拼写陷阱守护（commit 64edb48；6 tests，followup 9a833e9 dead_code 注释）
- [x] **T10** apps/card/{throttle,http}.rs throttle 400ms + Bearer + token_expired detect（commit ee87e07；4 tests，followup 668c1b5 truncate UTF-8）
- [x] **T11** apps/card/api.rs Client + get_balance + get_transactions（commit 3aed6ce；3 tests，followup e9312b0 card_no URL encode 防注入）
- [x] **T12** commands/card/{mod,data,handlers,refresh_helper}.rs（commit 456efc6；4 tests，followup 539b2e6 redact UTF-8 + 无效 ts warn）
- [x] **T13** cli/card.rs + cli/mod.rs Card variant 派发（commit 4d533f5；0 new tests，--help smoke 通过）

**累计**：13 主线 commits + 4 followup commits = 17 commits；新增 17 文件 + 修改 4 文件；~42 单测全过；clippy 0 warnings；fmt 通过；每文件 < 200 行（api.rs 200 恰好达限，最大近界文件）。

### 真机 CP 部分（T14-T16）— PENDING

- [ ] **T14** CP-T4-AUTH 首次授权 e2e — **阻塞**：等用户在 developer.sjtu.edu.cn 申请 client_id（scope: card_info + card_transactions，3 工作日审批）+ 把 client_secret 落 `~/.sjtu-cli/card_oauth_secret.txt`
- [ ] **T15** CP-T4-BAL / BAL-ID / HIST / HIST-EMPTY / LIMIT 5 项 — 依赖 T14
- [ ] **T16** CP-T4-REFRESH token 续期 — 依赖 T14（需挂 30+ 分钟让 token 过期）

### 收尾（T17）— 本提交

- [x] **T17** tasks/todo.md + tasks/lessons.md + README + SKILL + CLAUDE.md "当前阶段" 更新

### 已知 follow-up（不阻塞）

- ① **OQ-1 PKCE 服务端强制性**：spec 未实装 PKCE；T14 真机首跑若 server 报 PKCE required → 加一个 T7.5 task 加 code_challenge。否则保留 raw Authorization Code 路径
- ② **OQ-3 refresh_token 一次性策略**：jaccount 文档未明示 refresh_token 是否单次；T16 跑完 + 1-2 周连续使用观察是否 refresh 后 refresh_token 字符串改变
- ③ **elec/services/jwbmessage 接入 CAS retry 层**：S3e/S3a-c 当前没用 with_cas_refresh；待 T4 完成后系统化补
- ④ **elec/http.rs::truncate UTF-8 byte-slice 同款 bug**：card 端已修（commit 668c1b5），elec body 全 ASCII 未踩；当合并到 util 时统一修

---

## ⚪ S4 — 一卡通消费明细（从原 S4 降级为 S3e 拓展或 Phase 2）

> 从 2026-04-23 起推迟：基础余额查询归 S3e，消费明细留到 Phase 2。已由上方 S3 Phase 2 OAuth2 实现取代。

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
| 2026-04-26 | S3b archive-pm 实装 + delete-topic PM 预检 | 新增 `api_write::archive_pm`（PUT `/t/<id>/archive-message.json` + CSRF + finish_empty）+ `Client::archive_pm` + `cmd_archive_pm` + `ArchivePmData`；`models::TopicDetail` 加 `archetype: Option<String>`（`#[serde(default)]`）；`cmd_delete_topic` confirm 通过后先 `client.topic(id, 1)` 拿 archetype，是 `private_message` 时 `anyhow::bail!` 指向 archive-pm，避免 silent no-op；新增 `cli::shuiyuan::ShuiyuanSub::ArchivePm` + dispatch arm；现有 ShuiyuanSub doc 注释压缩使 `cli/shuiyuan.rs` 196 行（<200 硬限）；新增 2 mockito 单测（archive_pm 200 路径 + 4xx snippet）共 55/55 全绿；CP-PM2 自发自收 topic 469498 → sent returned 1→0；CP-DT-PM 对 PM topic 469500 跑 delete-topic → 友好错"请改用 archive-pm 469500"，不进 silent no-op；archive 469500 收尾 sent returned=0 | `cli/shuiyuan.rs` 还在 196 行紧邻 200 硬限，下次"清理一下"时建议把 ShuiyuanSub 拆到独立文件 |
| 2026-04-26 | S3a CP-W4 收尾 | new-topic 真机：`new-topic` 不传 `--category` 落 uncategorized → 水源自动重分类到"水源广场 谈笑风生"+ shuiyuan-bot 跟帖警告，返 `topic_id=469507/post_id=8805252/cooked` 三件套，**上行 post 路径已 verified**；`delete-topic 469507` → 422 "请与网站管理员联系"（水源 site-wide 对普通用户禁用 self-delete top-level topic，与 04-24 reply→delete-post 路径不同）；`delete-post` 在首楼 403（首楼保留）；75s 后重试仍 422 排除 per-minute 限流；web UI 弹窗确认是站点配置非 CLI bug。最终用户 web 手工 edit 标题→"加油喵～"/ 首楼→"加油做最好的自己" 无害化（CLI 没 edit-post 端点不能自动做）。lessons 加一条 "site-wide 禁用 self-delete + 测试帖 raw 必须伪装"，规则：水源测试帖永远不写 sjtu-cli/CP-W/自动化字样 | CLI 缺 `PUT /posts/<id>.json` edit-post 端点（一次性需求性价比低，未做）；CP-W4 收尾让 469507 留在水源（已无害化）；下次水源写测试默认 `--category` 钉一个允许 self-delete 的版块或干脆不验 delete 路径 |
| 2026-04-27 | S3d/S3e 方案锁定 | 把 S3d 办事大厅 / S3e 生活服务两条 ⚪ 线的执行方案落到 todo.md：S3d 先行（仿 S3f i.sjtu 半自动 SOP），MVP 缩小到"待办列表"一项，分 5 个 checkpoint CP-D0..CP-D4（调研→契约固化→用户确认→实装→真机）；S3e 顺次跟在 S3d 之后，分 4 个 checkpoint CP-E0..CP-E4，金额硬约束 `rust_decimal::Decimal`；硬红线（无写按钮 / read-only chrome-devtools / 只当只读访客）作为前置条款重申；`isjtu_investigation.md` §3（办事）/ §4（生活）章节占位待 CP-D1/CP-E1 时填入 | 调研尚未开始，等用户配合 chrome-devtools MCP 触发 CP-D0 |
| 2026-04-27 | S3d CP-D0/D1 ✅ | 半自动 SOP 抓三端点（用户切 tab / 我抓 network）：`/api/task/me/processes/todo?thing=false`（待办，total=1）/`/api/task/me/processes/completed?limit=10&start=0&order=auto&keyword=`（已办，total=37）/`/api/task/me/processes/cc?limit=10&start=0`（抄送，total=0）；envelope `{errno,error,total,entities}` 三端点共享；entity schema 差异**关键发现**：todo 嵌套 `process` 子对象 + 当前步骤铺顶层（`code`/`name`/`uri`/`assignTime`），completed/cc 直接铺平 + 多 `sort`/`pendingTasks`；my.sjtu 自家 REST，**非 ZF SP**，不复用 jwc HTTP helper；纯 Cookie 鉴权（JSESSIONID + keepalive）+ `X-Requested-With: XMLHttpRequest` header；契约写入 `isjtu_investigation.md` §6（§3-5 已被 i.sjtu SP 占用） | CP-D2 用户确认契约后才能进 D3 实装；completed 端点的"只看我申请的"/"只看进行中"过滤参数未现场捕获，phase-2 实装时再抓；S2 `cookies` 模块需先给 my.sjtu 加 sub-session 域；身份字段 owner.name/owner.id 默认脱敏 |
| 2026-04-27 | S3d CP-D2/D3 ✅ | CP-D2 用户口头确认契约通过；CP-D3 实装完成 — `apps/services/{mod,api,http,models,throttle,tests_parse}.rs`（6 文件，最长 161 行 / mod 18 行）+ `commands/services/{mod,handlers,data}.rs`（3 文件）+ `cli/services.rs`（34 行），全部 < 200 行硬限；**关键复盘**：之前 §6.6 / todo.md 写"cookies 模块需先给 my.sjtu 加 sub-session 域"是误判，sub-session 是按 name 文件名隔离（services.json vs jwbmessage.json），`cas_login("services", "https://my.sjtu.edu.cn/ui/app/")` 直接复用即可；不复用 jwc SP HTTP helper（独立 http.rs 142 行，结构同 jwbmessage::http.rs）；`code=="ADD"` 在 handler `partition` 拆 my_applications/awaiting_my_action；`--with-identity` 控脱敏；新增 `Services` clap variant + dispatch arm；`cargo fmt` / `clippy --all-targets -- -D warnings` / `test --lib` (64 passed，新增 4 个 services parse 单测) / `build --release` 全绿；`sjtu services --help` / `sjtu services pending --help` 渲染 OK | CP-D4 真机待用户私下触发 `sjtu services pending [--json|--yaml]`；`apps/jwc/api.rs:130` cargo fmt 顺手修了一处既存的 `time` 链式分割（pre-existing drift，不是我引入） |
| 2026-04-30 | S3e CP-E0/E0.1/E1 ✅ | 半自动抓 `elec.sjtu.edu.cn` 9 端点（`/api/me/info` + `/api/ws/sydl` + `/api/rechage/{ydl,ydlmx}` + 房间字典 5 链）；envelope 与办事大厅同款；金额混合类型（sydl 字段 string，ydl/ydlmx number）需 CLI 端 Decimal 统一；`/api/rechage/ydl` total:0 是服务端误标；CP-E0.1 决断：交我办 web 无一卡通 SP（taskcenter:// 移动 deep-link），ecard.sjtu.edu.cn 校网限制，**S3e MVP 仅做电费，一卡通延 phase-2**；契约写入 `isjtu_investigation.md §7`（占 §6 后续，§4-5 已被 i.sjtu 占）；硬红线全程未触"立即支付" | CP-E2 用户确认契约 + Decimal 路径 + 一卡通延期决断后才进 E3 实装 |
| 2026-04-30 | S3e CP-E2/E3 ✅ | CP-E2 用户口头确认通过；CP-E3 实装完成 — `apps/elec/{mod,api,http,models,throttle,tests_parse}.rs`（6 文件，最长 144 行）+ `commands/elec/{mod,handlers,data}.rs`（3 文件）+ `cli/elec.rs`（46 行），全部 < 200 行硬限；`Cargo.toml` 加 `rust_decimal = { version = "1", features = ["serde"] }` 做金额硬约束；CAS 入口 `cas_login("elec", "https://elec.sjtu.edu.cn/")` 独立 sub-session（不与 my.sjtu / jwc 共 cookie）；**关键复盘 1**：金额混合 string/number 用 `models::decimal_str_or_num` 自定义 `deserialize_any` Visitor 统一收成 `Decimal`，输出全走 `serialize_str` 避开 JSON f64 精度坑；**关键复盘 2**：generic `Envelope<T>` 默认派生 `Default` + `#[serde(default)]` 在 `Vec<T>` 上推断了 `T: Default`，把 `Balance/Monthly/DailyUsage` 也牵连，去 `Default` derive 不够，加 `#[serde(bound(deserialize = "T: serde::Deserialize<'de>"))]` 显式重写才彻底；**关键复盘 3**：`Envelope<T>` 只留 `entities` 一字段，errno/error/total 删掉避免 dead_code（`/api/rechage/ydl` total:0 误标本来就不能信）；新增 `Elec` clap variant + dispatch arm；`cargo fmt` / `clippy --all-targets -- -D warnings` / `test --lib` (71 passed，新增 6 个 elec 单测) / `build --release` 全绿；`sjtu elec balance/usage/history --help` 渲染 OK | CP-E4 真机待用户私下触发；房间未绑定 case (isbind=false) 真机未验；超 30 天 history 未验 |
| 2026-04-30 | S3e CP-E4 ✅ | 用户在校园网 Windows 11 cmd 跑通 `sjtu elec balance` / `usage` / `history --days 7` 三件套，逐项契约校验全过：`balance` 房间 D10-406 / `isbind:true` / 4 个金额字段全部字符串形式（`'284.25'` 不是 `284.25`，避开 f64 精度坑）；`usage` 当月 2026-04 / `last:80.55` / `now:62.44`（`total:0` 误标已忽略，CLI 只读 `entities[0]`）；`history` 区间 `2026-04-24`~`2026-04-30` returned 6（服务端漏当天 04-30，与 §7.6 文档预期一致），`total_kwh:'18.88'` Decimal 累加精确（3.37+2.16+3.78+2.40+1.94+5.23 = 18.88 手算复核 OK）；身份字段（name/work_no/account/dept）默认全部抹掉，输出只保留 `room` 位置标识；**关键复盘 4**：用户最初跑 `sjtu elec balance` 报 `'sjtu' 不是内部或外部命令` —— release binary 在 `target/release/sjtu.exe` 但未装到 PATH，给两条解法：相对路径直跑 / `cargo install --path . --locked`；S3e MVP 收尾 | 一卡通余额 / 消费明细 / 校车信息（如有）仍是 phase-2；`isbind:false` case 未真机验；`--days > 30` 范围未验 |
| 2026-05-12 | V5.F 撤回 audio-only 整路 ✅ | V5.B/D/E-B+ 三轮 audio-only 优化全失败（mp4 chunk-Range / sample-Range / 4-Client H2 池 + P85 自适应），最后一轮反向退化 21.6 min/讲（vs V5.A baseline 18 min/9 讲）；spec/plan/inline 执行三步落 V5.F 决策：删 `apps/canvas_video/{audio_dl,m4a_mux,mp4_box}/` 3 目录共 17 文件 1500 行 + 缩 `download_shared.rs` 128→90 行 + `data.rs` 清 V5.D 注释 + `mod.rs` 删 3 个 pub mod，2 commits 共 -2092 / +90；91 unit tests 全绿 + clippy `-D warnings` 零警告 + fmt 零 diff；真机：B.1 ffmpeg stdin pipe 5 min hexdump 验证否决（SJTU CDN mp4 是 moov-end 不可 stdin seek）；L10 单讲 smoke 1.74 min（target ≤ 2.5 min，all-pass）；9 讲 batch 907984 ms = 15.13 min（target ≤ 25 min 余量 40%）/ 9/9 mp4-full / 0 failed / 7.86 GB 总下载 / 全部 mp4_kept=false + audio_path 落盘 / 各讲 1.49-2.25 min 全过单讲阈值；lessons 加 5 条规则（CDN 真实约束验证 / H2 throughput-bound vs RTT-bound / 实验路径关 fail-soft / 优化走 sidetrack / 撤回按绝对值）；CLAUDE.md 当前阶段同步 V5.F | sub_sessions/canvas_video_bootstrap session ttl 仍 1800s 不变（V5.F 不动 LTI 链路）；后续 V6+ 若再做 audio-only 需走并行 worktree 不污染主线；Windows ACL 仍 TODO（S0 留白） |
| 2026-05-13 | OAuth2 path staleness fix ✅（补 5878fba 同源债） | `sjtu shuiyuan new-topic` 真机 403 → 严格核实 bug：`oauth2/mod.rs:46` cache hit 只判 `!sess.is_expired()`，没 `captured_at >= main.captured_at`，5878fba 修 CAS 时漏覆盖 OAuth2 path；shuiyuan.json captured_at=`2026-04-23` 比主 session `2026-05-12` 早 19 天但软 TTL=30d 未到 → 复用 stale `_t` 死循环；patch：`oauth2/mod.rs` 提前 load main session + 把 cache hit 判定换成 `cache_is_fresh(&sub, &main)` 复用 `cas::cache_is_fresh`；加 2 个 wiring unit test（stale 拒命中 / 同期或新接受）；端到端真机验证：retry new-topic 触发 OAuth2 重做，shuiyuan.json captured_at 刷新为 `2026-05-13T02:41:05`、Discourse 返 cooked HTML（topic 发出）；lessons 加 1 条 invariant（共享 staleness 函数必须覆盖所有 auth 入口）+ 4 条 reviewer 规则；fmt/clippy/test/release build 全绿 | T2.x"sub_session auto-invalidate on login_slogin.html" jwc-CAS 侧仍 TODO（CAS 入口 final_url 检测 + 自动 invalidate + 重试 1 次，与 OAuth2 path 不同语义层） |
| 2026-05-17 | T4 一卡通 公网调研收口 + spec 起稿 | 公网 curl 真机：ecard 302→restrict（公网不可达，**路径 A 永久放弃**）/ card.sjtu→weixin H5 入口非 REST / **api.sjtu.edu.cn/v1/me/card 公网 200** + envelope `{errno,error,total,entities}` 与 elec 同款 / developer.sjtu.edu.cn/api/card.html 完整契约可读；3 处修正 5-15 预研偏差：①余额端点是 `/v1/me/card` 不是 `/v1/me/card/user` ②`cardBalance/transBalance/amount` 全部 **`double`** 不是 string，必走 `decimal_str_or_num` 转 Decimal ③envelope 复用 elec generic `Envelope<T>` 无需新；OAuth2 完整文档抓全：`jaccount.sjtu.edu.cn/oauth2/{authorize,token,logout}`，token 1800s expires_in + refresh_token 续期，A 模式支持 `card_info`+`card_transactions` 两 scope；写两份文档：①`research/2026-05-17-t4-update.md`（180+ 行公网调研补丁 + 申请引导 + Decimal/PII/红线决策）②`specs/2026-05-17-t4-ecard-oauth2-design.md`（14 节 ~25KB，含 17 文件 ~1260 行 行数预算 + 12 mockito 测试矩阵 + 7 真机 CP + 6 R/4 OQ + Decimal helper 提取到 util/decimal 单一来源 + with_token_refresh 同构 canvas_video）；用户决策落定：新模块 `src/auth/oauth2_dev/` + redirect_uri `http://127.0.0.1:45123/callback` 固定端口；用户去 developer.sjtu.edu.cn 申请 client_id（3 工作日），同期我写 plan + TDD 代码骨架 | clientId 申请未提交；plan 文档未起；headless_chrome vs 系统默认浏览器二选一未定（spec 首版选 headless_chrome 复用主 session）；OQ-1 PKCE 服务端是否强制需审批后首跑确认；OQ-2 redirect_uri 平台是否允许 127.0.0.1 通配未定（spec 选固定端口 45123 防御此风险） |
| 2026-04-25 | S3a/S3b 真机 CP 验收 | 8/8 真机 checkpoint 全过：CP-1 login-probe → `authenticated:true` `from_cache:true` `elapsed_ms=6` `current_user.id=72509`；CP-2 latest --limit 3 → `returned=3`；CP-3 topic 468808 --post-limit 5 → `posts[0].post_number=1` `username=Narrenschiff`；CP-4 inbox --unread-only → `returned=6`；CP-5 search "jaccount" --in post → `posts_count=50`；CP-6 二次 login-probe → `elapsed_ms=6 < 100`；CP-M1 messages --filter inbox → `username=<水源昵称>` `returned=2`；CP-M2 message 404691 三 render 模式 (plain/markdown/raw) 语义全对（plain 剥 md / markdown==raw 保留）；**根因诊断**：本次卡 30+ 分钟全因 release binary 是 2026-04-23 16:55 编的旧版本（缺 `apps/shuiyuan/http.rs` 的 `pool_idle_timeout(0)` + `http1_only` 等修复）→ `cargo build --release` 重编后立刻通；本机网络须设 `HTTPS_PROXY=http://127.0.0.1:10808`（Clash mixed port），直连 DNS 解析水源超时；新增 `examples/proxy_diag.rs` 三组 builder 对照实验 `Default / Proxy::all / no_proxy + sjtu builder` 已删 | 写端点 CP-W4 (new-topic) 真机未触发；S3b pm-send 写端点未实装；S3b 交我办消息中心 SP 调研未做（待用户配合 chrome-devtools MCP）；S3d 办事大厅 / S3e 生活服务尚未启动 |
