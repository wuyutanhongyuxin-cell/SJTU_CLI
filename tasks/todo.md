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
- [x] **人工验证**：`sjtu login` 扫码成功 → 抓到 7 条 SJTU cookie 含 `JAAuthCookie`（前 8 位 `CDAHknZ6`）；`sjtu status` 读出 `authenticated: true / is_expired: false`，5 条脱敏展示
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

## ⚪ S3 — 教务（MVP 核心）

预估 2-3 天。

- [ ] 创建 `src/apps/jwc.rs`：课表、成绩 API 封装（scraper 解析 HTML）
- [ ] 创建 `src/models/course.rs`、`src/models/grade.rs`（`#[derive(Serialize, Deserialize)]`）
- [ ] 创建 `src/commands/schedule.rs`：`schedule` / `today` / `week` / `next`
- [ ] 创建 `src/commands/grades.rs`：`grades` / `gpa`
- [ ] 实现教学周计算（开学日期放 `config.toml`）
- [ ] 实现 `sjtu today` → 返回今天课表
- [ ] 实现 `sjtu next` → 下节课倒计时（chrono）
- [ ] `tests/jwc.rs`：mock 教务处 HTML
- [ ] **Checkpoint：`sjtu today --yaml` 输出当天课程**

---

## ⚪ S4 — 一卡通

预估 1 天。

- [ ] 创建 `src/apps/card.rs`
- [ ] 创建 `src/models/card_record.rs`（金额字段 `rust_decimal::Decimal`）
- [ ] 创建 `src/commands/card.rs`：`card` / `card history`
- [ ] `tests/card.rs`
- [ ] **Checkpoint：`sjtu card` 显示余额**

---

## ⚪ S5 — 通知 + Canvas

预估 2 天。

- [ ] 创建 `src/apps/notifications.rs`
- [ ] 创建 `src/apps/canvas.rs`（优先 Canvas REST API + Personal Access Token）
- [ ] 创建 `src/models/notification.rs`
- [ ] 创建 `src/commands/notifications.rs`：`notifications` / `notifications --unread`
- [ ] 创建 `src/commands/canvas.rs`：`canvas assignments` / `canvas todo` / `canvas grades`
- [ ] 通知去重：`(source, notification_id)` 复合键，`HashSet<(String, String)>`
- [ ] **Checkpoint：`sjtu canvas todo` 显示未来 7 天 DDL**

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
