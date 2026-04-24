# SJTU-CLI Next Steps（2026-04-23 起）

> 本文档展开 `tasks/todo.md` 里 S3a–S3e 的细节。每个子阶段独立章节，含目标、依赖、端点、Checkpoint、文件清单。
>
> 路线图回顾（用户钦定）：**S3a 水源 → S3b 消息中心 → S3c Canvas 作业 DDL → S3d 办事 → S3e 生活服务**；只读先行；写操作默认 `--confirm`。
>
> 2026-04-24：S3c 原为"交我办日程"占位，经用户钦定缩小 scope 为 **Canvas 作业 DDL**。交我办日程 / jwc 课表 / 聚合日历留 Phase 2。

---

## 🟡 S3a-续 水源社区真实 checkpoint（当前卡点）

### 目标
把已经写好的 5 个只读命令（`latest` / `topic` / `inbox` / `search` / 隐藏 `login-probe`）真实跑通水源 shuiyuan.sjtu.edu.cn，产出手动验证截图 / 输出样本，把代码状态从"已编译"提升到"已实战验证"。

### 前置条件
- 本机 `sjtu login` 已跑过（`session.json` 含有效 JAAuthCookie）。若过期，用户需要重跑 `sjtu login` 扫码。
- 目录 `C:/Users/16191/AppData/Roaming/sjtu-cli/` 存在（首次 `sjtu login` 会自动创建）。

### Checkpoint

| 编号 | 命令                                              | 预期                                                                                        |
| ---- | ------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| CP-1 | `sjtu shuiyuan login-probe --yaml`                | `data.authenticated: true` + `data.from_cache: false`（首次）/ `true`（二次）+ `current_user.username` 是用户的 jAccount |
| CP-2 | `sjtu shuiyuan latest --limit 3 --yaml`           | `data.returned: 3` + topics[0..3] 每条含 `id` / `title` / `posts_count` / `views`           |
| CP-3 | `sjtu shuiyuan topic <id from CP-2> --post-limit 5 --yaml` | `data.posts_count` > 0，`posts[0]` 含 `post_number=1` + `username` + `body` 非空      |
| CP-4 | `sjtu shuiyuan inbox --unread-only --yaml`        | `data.unread_only: true`，`data.returned` ≥ 0（空列表也算通过）                            |
| CP-5 | `sjtu shuiyuan search "jaccount" --in post --yaml` | `data.posts_count` ≥ 1，每条含 `topic_id` / `username`                                     |
| CP-6 | 二次运行 `login-probe`                            | `data.from_cache: true` + `elapsed_ms` < 100（缓存命中加速）                                 |

### 可能翻车点 + 应对
- **403/401**：水源 Discourse 对 UA 敏感；`src/apps/shuiyuan/api.rs` 里已用 Chrome/124 UA。若仍 403，可能是 cookie 过期 → `oauth2_login` 会自动刷，刷完仍 403 再看 snippet。
- **OAuth2 链路停在 jaccount.sjtu.edu.cn**：JAAuthCookie 失效，需要 `sjtu login` 重扫。
- **rookie 兜底静默跑到 Firefox 路径**：`via_rookie_fallback: true` 会暴露；若用户没装 Chrome/Edge/Firefox 任何一个已登录 jaccount，会 fail fast。
- **Discourse 限流（200 req/min）**：已加 300 ms throttle；连跑 5 个命令不会触发。

### 交付
- 运行录屏 / 截图 / CLI 输出样本 贴进 `tasks/todo.md` 进度记录行
- 有 bug 立即补丁，不留 TODO；改完重跑 CP-1..CP-6 全绿
- `cargo test` 仍 25/25（不回归）

**估时**：30-60 分钟（含一次扫码）

---

## 🟡 S3a-写 水源写操作（reply / like / new-topic）

### 目标
在 S3a 真实 checkpoint 全绿后，加水源社区写操作。**每个写操作默认 `--confirm`**：直接加参数会先打印"即将执行 XX"并二次确认（stdin y/n），只有 `--yes` 才跳过确认（给脚本化用）。

### 依赖调研
- Discourse 写操作需要 CSRF token：`GET /session/csrf.json` → `{"csrf":"<token>"}`
- 所有 POST / PUT / DELETE 请求头带 `X-CSRF-Token: <token>` + `X-Requested-With: XMLHttpRequest`

### 端点清单

| 命令                                                                  | 方法 | 路径                                  | Body                                                   |
| --------------------------------------------------------------------- | ---- | ------------------------------------- | ------------------------------------------------------ |
| `sjtu shuiyuan reply <topic_id> --body <text> [--confirm/--yes]`      | POST | `/posts.json`                         | `raw=<text>&topic_id=<id>&nested_post=true`            |
| `sjtu shuiyuan like <post_id> [--confirm/--yes]`                      | POST | `/post_actions`                       | `id=<post_id>&post_action_type_id=2&flag_topic=false`  |
| `sjtu shuiyuan new-topic --category <id> --title <t> --body <b> [...]` | POST | `/posts.json`                         | `title=<t>&raw=<b>&category=<id>`                      |

### 文件清单
- `src/apps/shuiyuan/api.rs` 新增 `reply` / `like` / `new_topic` / `csrf_token`（可能要拆 `api_write.rs` 守 200 行限制）
- `src/commands/shuiyuan/handlers.rs` 新增 `cmd_reply` / `cmd_like` / `cmd_new_topic`（接近 200 行时再拆）
- `src/cli/shuiyuan.rs` 新增 3 个 `ShuiyuanSub` variant
- `src/commands/mod.rs` 如有新模块同步导出
- 新增 `src/util/confirm.rs` 或直接复用已有的 confirm helper（如无则写一个：stdin prompt + y/n）

### Checkpoint
| 编号  | 命令                                                       | 预期                                                                 |
| ----- | ---------------------------------------------------------- | -------------------------------------------------------------------- |
| CP-W1 | `sjtu shuiyuan reply <一个测试 topic> --body "hello from cli"`（无 `--yes`） | 先打印"即将回复 topic N..."，输入 n → 不发送，Envelope `aborted: true` |
| CP-W2 | 同上，输入 y                                               | 返 `post_id` + `post_number`，在水源能看到该楼                       |
| CP-W3 | `sjtu shuiyuan like <上面 post_id> --yes`                  | 免交互，返 `liked: true`；再跑一次返 `already_liked: true`           |
| CP-W4 | `sjtu shuiyuan new-topic --category <灌水区> --title X --body Y --yes` | 返 `topic_id` + `topic_url`                                          |
| CP-W5 | `cargo test` 新增 mockito 写测试                           | 新增至少 3 个测试（CSRF 获取 + POST 路径 + --confirm 中断）          |

**估时**：0.5 天

---

## ⚪ S3b 消息中心

### 目标
让 Claude 能一行命令查看"交我办 APP 消息中心"的推送：通知、审批提醒、课程公告。

### 调研项（写代码前必须先做）
- [ ] 用户登录"我的交大"后，打开"消息中心"，抓浏览器 DevTools 里 XHR 请求，记 SP URL / API 路径 / 返回 JSON 形状
- [ ] 判断走哪条链：
  - 若是独立 SP → CAS 通道（复用 S2 `cas_login`）
  - 若挂在 my.sjtu.edu.cn 下 → 主 session 直接用（cookies.json 即可）
  - 若是 OAuth2 应用 → 复用 S3a 的 `oauth2_login`

### 端点（占位，等调研填实）
```
GET /<sp-host>/msg/list?unread=true
GET /<sp-host>/msg/<id>
POST /<sp-host>/msg/<id>/read
```

### CLI 设计
- `sjtu messages [--unread] [--limit N]`：列消息摘要
- `sjtu messages show <id>`：看单条正文
- `sjtu messages mark-read <id> [--yes]`：标记已读（写操作要 `--confirm`）
- `sjtu messages mark-all-read --yes`：批量标记（高风险，强制 `--confirm`）

### 文件清单
- `src/apps/messages/{mod,api,models,tests}.rs`
- `src/commands/messages/{mod,data,handlers}.rs`
- `src/cli/messages.rs`（clap ValueEnum + dispatch）
- `src/cli/mod.rs` 加 `Messages { sub: MessagesSub }` 顶层 variant

### Checkpoint
| 编号    | 命令                                         | 预期                                                                |
| ------- | -------------------------------------------- | ------------------------------------------------------------------- |
| CP-B1   | `sjtu messages --unread --limit 5 --yaml`    | `data.returned` ≥ 0，每条含 `id` / `title` / `sender` / `created_at` |
| CP-B2   | `sjtu messages show <id from CP-B1>`         | 返消息正文（纯文本 / Markdown 渲染）                                |
| CP-B3   | `sjtu messages mark-read <id> --yes`         | `marked: true`；再跑 CP-B1 该 id 消失                               |

**估时**：1-2 天（调研是大头）

---

## ⚪ S3c Canvas 作业 DDL

### 目标
一行命令出"今天 / 未来 N 天"的 Canvas 作业截止时间。给 Claude 用来回答"还有哪些没交 / 下一个 DDL 什么时候"。
**Scope（2026-04-24 用户钦定）**：只做 Canvas 作业 DDL，不做交我办日程 / jwc 课表 / Canvas 收件箱 / 公告 / 成绩 —— 留 Phase 2。

### 调研结论（2026-04-24 chrome-devtools MCP 实抓 `oc.sjtu.edu.cn`，详见 `tasks/s3c-canvas-planner.md`）
- [x] Canvas SP URL = `oc.sjtu.edu.cn`；响应 `application/json; charset=utf-8`；HTTP/1.1 + gzip
- [x] 鉴权首选 **Personal Access Token**（CLAUDE.md 项目钦定）—— `Authorization: Bearer <PAT>`，永不过期除非手动 revoke；次选 JAccount SAML SSO cookie（留 Phase 2 兜底）
- [x] 核心端点 = `GET /api/v1/planner/items` —— 单端点聚合所有课程 DDL，带 `plannable_date`（UTC）+ `submissions.*` + `context_name`，按 `start_date` / `end_date` 过滤
- [x] 无 CSRF（全只读）；rate limit 600 单位桶（`x-rate-limit-remaining` header）；分页 `Link: rel="next"`（`per_page` 默认 10、上限 100）
- [x] iCal 方案 (`/profile.calendar.ics`) 确认存在但**不走**：需新 crate `icalendar`，planner/items 已够用

### 端点清单

| 用途 | 方法 | 路径 | 关键参数 |
|---|---|---|---|
| whoami | GET | `/api/v1/users/self` + `/api/v1/users/self/profile` | — |
| today | GET | `/api/v1/planner/items` | `start_date`=今天 00:00 本地→UTC、`end_date`=明天 00:00 本地→UTC |
| upcoming | GET | `/api/v1/planner/items` | `start_date`=今天 00:00 本地→UTC、`end_date`=`start_date + --days`（默认 14）、`order=asc` |

固定请求头：`Authorization: Bearer <PAT>` + `Accept: application/json+canvas-string-ids, application/json` + `X-Requested-With: XMLHttpRequest`。

### CLI 设计
- `sjtu canvas setup`：交互式粘贴 PAT，落盘 `~/.sjtu-cli/sub_sessions/canvas_token.txt`（600 权限）
- `sjtu canvas whoami [--yaml]`：验 PAT 有效，返 `login_id` / `time_zone` / `effective_locale`
- `sjtu canvas today [--include-done] [--yaml]`：今日作业（默认只显未交未评，加 flag 显全部）
- `sjtu canvas upcoming [--days N=14] [--include-done] [--yaml]`：未来 N 天作业

**不做**（Phase 2）：`canvas courses` / `assignments <course>` / `inbox` / `announcements` / `grades`；所有写端点。

### 文件清单（对齐 `src/apps/shuiyuan/` 与 `src/apps/jwbmessage/` 骨架）
- `src/apps/canvas/{mod,api,http,models,throttle,auth,tests_parse}.rs`（文件数 ≥ 3 加 README.md）
- `src/commands/canvas/{mod,data,handlers}.rs`
- `src/cli/canvas.rs`（clap `CanvasSub` 枚举 + dispatch）
- `src/cli/mod.rs` 加 `Canvas { sub: CanvasSub }` 顶层 variant
- **PAT 落盘用独立文件** `~/.sjtu-cli/sub_sessions/canvas_token.txt`，**不扩** `Session` struct（PAT 不是 cookie，避免污染模型）

### Checkpoint
| 编号 | 命令 | 预期 |
|---|---|---|
| CP-C1 | `sjtu canvas setup`（粘 PAT 后） + `sjtu canvas whoami --yaml` | `data.login_id` = 用户 jAccount + `data.effective_locale: "zh-Hans"` + `data.time_zone: "Asia/Shanghai"` |
| CP-C2 | `sjtu canvas today --yaml` | `data.returned ≥ 0`；若非空每条含 `title / course / due_at_local / points / submitted` |
| CP-C3 | `sjtu canvas upcoming --days 14 --yaml` | 按 `due_at` 升序；`data.total ≥ today.returned` |
| CP-C4（可选）| 将 `canvas_token.txt` 改为无效值后再跑 `canvas whoami` | 401 分支触发 Envelope `error.code: session_expired` + 提示 `重跑 sjtu canvas setup` |

### 依赖预报
- `chrono`（已在 S0 声明）：UTC ↔ `Asia/Shanghai` 换算
- **不引入**新 crate（iCal 路线推迟；PAT 走现有 `reqwest` 的 `bearer_auth`）

**估时**：0.5–0.8 天（调研 0.2 已花，剩代码 + 3 个 CP 对点）

---

## ⚪ S3d 办事大厅

### 目标
让 Claude 查"办事大厅"里的待办事项、已办、可发起流程。先只读，写操作（发起新流程）留到 S3d 后期。

### 调研项
- [ ] 办事大厅 SP URL（`ehall.sjtu.edu.cn` 或类似）
- [ ] 流程列表接口（`getTodoList` / `getPendingItems` 之类）
- [ ] 单条流程详情接口

### CLI 设计（只读先）
- `sjtu services pending [--yaml]`：我的待办
- `sjtu services history [--limit N]`：已办历史
- `sjtu services search <keyword>`：搜索可发起事项
- `sjtu services show <flow_id>`：看某条流程详情

### 文件清单
- `src/apps/services/{mod,api,models,tests}.rs`
- `src/commands/services/...`
- `src/cli/services.rs`

### Checkpoint
| 编号  | 命令                                  | 预期                                                            |
| ----- | ------------------------------------- | --------------------------------------------------------------- |
| CP-D1 | `sjtu services pending --yaml`        | `data.pending[]` 每条含 `id` / `title` / `submit_time`           |
| CP-D2 | `sjtu services search "请假" --yaml` | `data.matches[]` ≥ 1                                             |
| CP-D3 | `sjtu services show <id>`             | 返流程节点历史 + 当前状态                                       |

**估时**：1-2 天

---

## ⚪ S3e 生活服务

### 目标
一卡通余额 + 宿舍电费查询。这是最"好用"的场景之一：Claude 能一行命令告诉我"卡里还剩 15 块"。

### 调研项
- [ ] 一卡通余额 SP URL（官方 `ecard.sjtu.edu.cn` 或"交我办"聚合）
- [ ] 电费查询 SP（学校有独立电费系统，或"交我办"里的宿舍板块）
- [ ] 是否有金额的 `string` 字段（避免 JSON f64 精度）

### CLI 设计
- `sjtu card balance [--yaml]`：一卡通余额
- `sjtu elec balance --dorm <id> [--yaml]`：宿舍电费
- （可选）`sjtu shuttle` 校车表

### 文件清单
- `src/apps/card/{mod,api,models,tests}.rs`（已占位）
- `src/apps/electricity/{mod,api,models,tests}.rs`
- `src/commands/card.rs` / `src/commands/electricity.rs`
- `src/cli/card.rs` / `src/cli/electricity.rs`

### 项目专属约束（来自 CLAUDE.md）
- 金额字段**必须**用 `rust_decimal::Decimal`，序列化为 string
- 禁止 f32 / f64

### Checkpoint
| 编号   | 命令                                      | 预期                                                                                                             |
| ------ | ----------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| CP-E1  | `sjtu card balance --yaml`                | `data.balance: "15.23"`（string）+ `data.currency: "CNY"` + `data.last_update: "2026-04-23T..."`                 |
| CP-E2  | `sjtu elec balance --dorm <id> --yaml`    | `data.remaining_kwh: "42.5"` + `data.dorm_id` / `data.building`                                                 |
| CP-E3  | `cargo test` 金额字段 Decimal 序列化单测  | 写 3 个单测：Decimal 零值 / 小数 / 大数 全部序列化为 string 不走 JSON number                                    |

**估时**：1-2 天

---

## 📦 S3 汇总 Checkpoint（所有子阶段做完后）

- [ ] 6 个子阶段（S3a 读 / S3a 写 / S3b / S3c / S3d / S3e）全部有真实 checkpoint 记录
- [ ] `tasks/todo.md` 进度记录 6 条新行
- [ ] `cargo test` 总数至少 35+（S3a 已 25，每阶段至少 +2）
- [ ] 任意文件均 < 200 行；任意目录下文件数 > 3 有 README.md
- [ ] 所有写操作默认 `--confirm`，有 mockito 测试证明"输入 n 不发送"
- [ ] CLAUDE.md 项目结构章节同步到实际目录
- [ ] 升级到 S6：补 `tests/smoke.rs` 用 `#[ignore]` 包装所有真实 API

---

## 🎯 元原则（贯穿 S3a-e）

1. **调研先行**：任何子阶段动代码前，先手动 DevTools 抓一遍接口 + 记录响应，再动手。抓不到直接退一步问用户。
2. **只读先行**：同一个子系统里先做 GET 命令，看到真实数据再上写操作。
3. **写操作默认 `--confirm`**：没有 `--yes` 就 stdin prompt；mockito 测 "n → aborted / y → 发请求"。
4. **文件拆分前置**：≥ 180 行立刻主动拆，不要撞 200 再拆（CLAUDE.md 硬限）。
5. **UA / 限流照抄水源**：所有 SJTU 子系统默认 Chrome UA + 300 ms throttle，除非上游官方 API 白名单。
6. **金额 Decimal**：任何带钱的字段（卡余额 / 电费 / 订单）全走 `rust_decimal::Decimal`，序列化为 string。
7. **失败 graceful**：401/403 → 自动刷 session 一次；还是失败 → Envelope `error.code=session_expired` + 提示重扫。
