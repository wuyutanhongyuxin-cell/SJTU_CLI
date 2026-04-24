# S3c Canvas 作业 DDL —— 调研结果

> 调研日期：2026-04-24（chrome-devtools MCP 实抓 `oc.sjtu.edu.cn`，账号脱敏）
> 方法：交我办 JAccount 会话 SSO 打通 Canvas dashboard → 拦 XHR → 反查 API 形状
> 状态：✅ 契约全部锁定；可直接进 MVP 实装（只读 3 命令：`whoami` / `today` / `upcoming`）
> 范围（用户钦定）：**仅作业 DDL** —— 不做 Canvas 收件箱、讨论、成绩、公告（留 Phase 2）

---

## 1. 链路归类

| 证据 | 观察值 |
|---|---|
| 入口 URL | `https://oc.sjtu.edu.cn/`（未登录落 `/login/canvas`，带 Canvas session cookie 时直落 dashboard） |
| SSO | 交我办 JAccount 主 session → Canvas `/login/saml` → 落 `/?login_success=1` 建 Canvas 本地 session（**本次未单独拆 SAML 链，因为用户已在另一 tab 持 Canvas session**） |
| XHR 域 | 全部 `oc.sjtu.edu.cn/api/v1/*`，无跨域 |
| 鉴权 cookie | `_normandy_session` + `_legacy_normandy_session`（关键）+ `_csrf_token`（双提交）+ `keepalive` + `log_session_id`（都只在 `oc.sjtu.edu.cn` 域，**不复用 JAccount cookie**） |
| 关键请求头 | `Accept: application/json+canvas-string-ids, application/json` + `X-Requested-With: XMLHttpRequest` |
| CSRF | 只在 POST/PUT/DELETE 需要：把 `_csrf_token` cookie 值 URI-decode 后写入 `X-CSRF-Token` header（Rails 双提交模式）。MVP 全只读，**不需要**。 |
| Bearer / PAT | Canvas 官方支持 `Authorization: Bearer <token>` —— 用户在 `/profile/settings` → Approved Integrations → New Access Token 生成，永不过期除非手动 revoke。**CLAUDE.md 项目专属提醒钦定 PAT 为 Canvas 首选鉴权** |
| Rate Limit | 响应头 `x-rate-limit-remaining: 600.0` —— 600 单位桶，按请求成本扣（观察 planner/items `x-request-cost: 0.17`，约 3500 次 /min 上限，足够用） |
| 分页 | Link header `rel="current" / "first" / "next" / "last"`；`per_page` 默认 10，上限 100（Canvas 全站规则） |

### 对实装的意义

- **鉴权双轨制**：
  1. **首选 PAT**（CLAUDE.md 钦定）—— 用户跑 `sjtu canvas setup`，粘贴手动生成的 token 存到 `~/.sjtu-cli/sub_sessions/canvas.json`；运行时发 `Authorization: Bearer <token>`。零 SSO 开销、session 不过期、幂等可重放。
  2. **次选：headless_chrome SSO 兜底**（用户不愿手动生成 token 时）—— 复用 `src/auth/qr_login.rs` 范式打 `oc.sjtu.edu.cn/` → 抓 Canvas session cookies → 落 sub_sessions；session 约 24h 过期后需重扫。
- **MVP 只做 PAT 这一条**，SSO 兜底留给后续子阶段（类似 S3a 先手工再自动）。
- Canvas session cookie **完全不共享** JAccount —— 本机装过别的 JAccount 工具（如 S3b 交我办 client）**不影响** Canvas。

---

## 2. 端点契约

所有端点都是 `GET`，响应 `application/json; charset=utf-8`，响应头固定带 `x-canvas-user-id` / `x-rate-limit-remaining` / `x-request-cost` / Link 分页。

### 2.1 `/api/v1/users/self` + `/profile`（whoami）

```
GET /api/v1/users/self
→ {"id":"<CANVAS_UID>","name":"张三","sortable_name":"张, 三","short_name":"张三",
   "avatar_url":"https://oc.sjtu.edu.cn/images/thumbnails/<...>","locale":null,
   "effective_locale":"zh-Hans","permissions":{...}}

GET /api/v1/users/self/profile
→ {"id":"<CANVAS_UID>","name":"张三","short_name":"...","sortable_name":"...",
   "avatar_url":"...","title":null,"bio":null,
   "primary_email":"<login>@sjtu.edu.cn","login_id":"<jaccount>",
   "integration_id":null,"time_zone":"Asia/Shanghai","locale":null,
   "effective_locale":"zh-Hans",
   "calendar":{"ics":"https://oc.sjtu.edu.cn/feeds/calendars/user_<TOKEN>.ics"},
   "lti_user_id":"<uuid>","k5_user":false}
```

**用途**：
- `login_id` 做用户名展示 + `cmd_login_probe` 验证 PAT 有效。
- `calendar.ics` 是个人 iCal feed，包含所有 assignment 的 `DTSTART/DTEND`。**这是一条替代路线**：用 `icalendar` crate 解析离线 ics 文件就能出"本周 DDL"——但依赖新 crate，**MVP 不走这条**。

### 2.2 `/api/v1/dashboard/dashboard_cards`（课程卡片）

```
GET /api/v1/dashboard/dashboard_cards
→ [
  {
    "id":"87026",
    "shortName":"马克思主义基本原理",
    "longName":"马克思主义基本原理 - 本-(2025-2026-2)-MARX1204-15-马克思主义基本原理",
    "originalName":"马克思主义基本原理",
    "courseCode":"本-(2025-2026-2)-MARX1204-15-马克思主义基本原理",
    "assetString":"course_87026",
    "href":"/courses/87026",
    "term":"2025-2026 Spring",
    "subtitle":"录取为： 学生",
    "enrollmentState":"active",
    "enrollmentType":"StudentEnrollment",
    "published":true,
    "isFavorited":false,
    "links":[{"css_class":"assignments","icon":"icon-assignment","path":"/courses/87026/assignments","label":"作业"}, ...],
    "defaultView":"modules"
  }, ...
]
```

**用途**：MVP 其实**不需要**单独调这个 —— planner/items 的每条响应都带 `context_name`（课程简称），够用。此端点留作 `sjtu canvas courses` 子命令的数据源，**不进 MVP**。

### 2.3 `/api/v1/planner/items`（★ MVP 核心：DDL 聚合）

```
GET /api/v1/planner/items?start_date=<ISO8601_UTC>[&end_date=<ISO8601_UTC>][&filter=new_activity][&order=asc|desc][&per_page=N]

Query:
  start_date    必传。ISO8601，UTC。前端用"今天凌晨 00:00 本地时间"换算成 UTC。
  end_date      可选。不传 = 一直到未来全部。
  filter        可选。`new_activity` = 只返有新动态（新评分 / 新评论）的。
  order         可选。asc（默认） / desc。
  per_page      默认 10，上限 100。

→ [
  {
    "context_type":"Course",
    "course_id":"88169",
    "plannable_id":"405484",
    "plannable_type":"assignment",               // 也可能：discussion_topic / quiz / planner_note / calendar_event
    "plannable_date":"2026-04-25T15:59:59Z",     // 关键排序字段（UTC）
    "new_activity":false,
    "planner_override":null,
    "submissions": {
      "submitted":false, "excused":false, "graded":false, "posted_at":null,
      "late":false, "missing":false, "needs_grading":false,
      "has_feedback":false, "redo_request":false
    },
    "plannable": {
      "id":"405484",
      "title":"【4月22日】提交演讲稿",
      "created_at":"2026-04-22T11:45:50Z",
      "updated_at":"2026-04-22T11:45:51Z",
      "points_possible":75.0,
      "due_at":"2026-04-25T15:59:59Z"             // 与 plannable_date 一致
    },
    "html_url":"/courses/88169/assignments/405484",
    "context_name":"日语演讲比赛（3）",
    "context_image":null
  },
  ...
]
```

**关键字段语义**：
- `plannable_date` vs `plannable.due_at`：作业类两者相等；`planner_note`（用户自建待办）只有 `plannable_date`。**排序一律用 `plannable_date`**。
- `submissions.submitted` = 已交；`missing` = 过期未交；`graded` = 已评分。CLI 筛选"未完成" = `!submitted && !excused`。
- `points_possible: 0.0` = 不计分作业（常见于讨论题 + "请完成" 类任务）。
- `plannable_type` 种类：
  - `assignment` —— 常规作业（有 due_at + points_possible）
  - `discussion_topic` —— 讨论区发帖要求（可能有 due_at）
  - `quiz` —— 测验
  - `planner_note` —— 用户自建的待办笔记（无 submissions 字段）
  - `calendar_event` —— 日历事件（无 submissions）
- 时区：所有时间字段都是 **UTC**。SJTU 本地时间需要 `+08:00`。

### 2.4 `/api/v1/users/self/missing_submissions`（未交作业）

```
GET /api/v1/users/self/missing_submissions?include[]=planner_overrides&filter[]=current_grading_period&filter[]=submittable

→ [] 或 [{...assignment 对象...}]
```

**用途**：MVP 可选 —— `planner/items` 已能覆盖（筛 `submissions.missing == true`），所以这个端点作为**二级确认**，不进 MVP。

### 2.5 `/api/v1/conversations/unread_count` + `/api/v1/release_notes/unread_count`

小徽标端点（侧边栏红点用），**不进 MVP**。

---

## 3. 其他顺带捕获（未进 MVP，但登在案）

| 端点 | 用途 | 可能对接子命令 |
|---|---|---|
| `GET /api/v1/courses` | 当前选课列表（带分页，可过滤 `enrollment_state`） | `sjtu canvas courses` |
| `GET /api/v1/courses/:id/assignments` | 单课程全部作业 | `sjtu canvas assignments <course>` |
| `GET /api/v1/courses/:id/assignments/:id` | 单作业详情（含 `description` HTML 正文） | `sjtu canvas assignment <course> <id>` |
| `GET /api/v1/conversations` | Canvas 站内信 | `sjtu canvas inbox` |
| `GET /api/v1/announcements?context_codes[]=course_<id>` | 课程公告 | `sjtu canvas announcements` |
| `GET /feeds/calendars/user_<TOKEN>.ics` | 个人 iCal feed（离线聚合） | 替代 `today`/`week` 的 offline 方案 |

---

## 4. 实装建议（MVP 前沿）

### 4.1 文件清单

对齐 `src/apps/shuiyuan/` 与 `src/apps/jwbmessage/` 的骨架，守 200 行硬限：

```
src/apps/canvas/
├── mod.rs                   # Client struct + connect() + 转发到 api.rs
├── api.rs                   # whoami / planner_items（读端点）
├── http.rs                  # reqwest Client 构造（注入 PAT Bearer + throttle）
├── models.rs                # PlannerItem / Plannable / Submissions / Profile struct
├── throttle.rs              # 300ms 固定间隔（保守，Canvas 600 bucket 已宽松）
├── auth.rs                  # PAT 落盘 / 读取 / 失效检测（401）
├── tests_parse.rs           # serde 解析测试（fixtures 取自本文 §2）
└── README.md                # 目录说明（CLAUDE.md §2 硬要求，文件≥3 时）
src/commands/canvas/
├── mod.rs
├── data.rs                  # Envelope Data 结构
└── handlers.rs              # cmd_whoami / cmd_today / cmd_upcoming / cmd_setup
src/cli/canvas.rs            # clap Subcommand（CanvasSub 枚举 + dispatch）
```

### 4.2 CLI 命令（MVP）

```
sjtu canvas setup                  # 交互式粘贴 PAT → 写 sub_sessions/canvas.json
sjtu canvas whoami                 # GET /api/v1/users/self + /profile；验证 PAT 有效
sjtu canvas today                  # GET planner/items?start_date=<今天 00:00 本地>&end_date=<明天 00:00 本地>
  --include-done                   # 默认只显未交 / 未评；加此 flag 显全部
sjtu canvas upcoming               # GET planner/items?start_date=<今天 00:00>
  --days N                         # 默认 14；限制 end_date = 今天 + N 天
  --include-done
```

**不做的命令**（作业 DDL 范围外）：
- `sjtu canvas courses` / `assignments` / `inbox` / `announcements` —— 留 Phase 2
- **所有写端点** —— MVP 不提 submit / reply 类操作

### 4.3 鉴权层（PAT 主链路）

```rust
// src/apps/canvas/mod.rs（示意）
pub struct Client { http: reqwest::Client, throttle: Throttle, base: String }

impl Client {
    pub async fn connect() -> Result<Self> {
        let pat = crate::cookies::load_sub_session("canvas")?   // 复用 sub_session 落盘
            .and_then(|s| s.canvas_pat.clone())
            .ok_or(SjtuCliError::NotAuthenticated("先跑 sjtu canvas setup"))?;
        let http = build_http_client(&pat)?;                     // 注入 Authorization: Bearer <pat>
        Ok(Self { http, throttle: Throttle::new(), base: CANVAS_BASE.into() })
    }
    pub async fn whoami(&self) -> Result<Profile> { ... }
    pub async fn planner_items(&self, start: DateTime<Utc>, end: Option<DateTime<Utc>>) -> Result<Vec<PlannerItem>> { ... }
}
```

**`sub_sessions/canvas.json` 要加字段**：目前 `Session` struct 只有 cookies；Canvas PAT 不是 cookie。方案二选一：
- (A) **扩 `Session`**：加 `canvas_pat: Option<String>` —— 改一处、污染模型。
- (B) **单独文件**：`sub_sessions/canvas.toml` 或 `canvas_token.txt`（600 权限）—— 隔离、干净。
- **推荐 (B)**。实装时新增 `src/apps/canvas/auth.rs` 管这个文件，不动 `cookies/` 模块。

### 4.4 Checkpoints（MVP 对点）

| 编号 | 命令 | 预期 |
|---|---|---|
| CP-C1 | `sjtu canvas setup`（粘贴 PAT 后） + `sjtu canvas whoami --yaml` | `data.login_id` / `data.effective_locale: "zh-Hans"` / `data.time_zone: "Asia/Shanghai"` |
| CP-C2 | `sjtu canvas today --yaml` | `data.returned ≥ 0`；若有作业则每条含 `title / course / due_at_local / points / submitted` |
| CP-C3 | `sjtu canvas upcoming --days 14 --yaml` | 按 `due_at` 升序；`data.total` ≥ `today.returned` |
| CP-C4（可选）| `sjtu canvas whoami`（故意改 PAT 为无效值后） | 401 分支触发 Envelope `error.code: session_expired` + 提示 `重跑 sjtu canvas setup` |

### 4.5 输出字段本地化

- `due_at` UTC → 本地化为 `Asia/Shanghai` 的 `YYYY-MM-DD HH:MM`（前端展示用）
- 保留 `due_at_utc` 原值供 AI Agent 计算相对时间
- `points_possible: 0.0` 显示为 `"不计分"`，其他显示 `"75 分"`

---

## 5. 对 plan-next.md 的回写清单

调研完成后需要改 `tasks/plan-next.md §⚪ S3c 日程`：

1. **改标题**：`S3c 日程` → `S3c Canvas 作业 DDL`（用户钦定缩小 scope，"日程"留给未来的 Phase 2 —— 届时可追加 `/api/calendar/today` 交我办日程 + jwc 课表 + Canvas 作业 的聚合命令）。
2. **替换调研项列表**：占位三条全打勾（Canvas SP URL = `oc.sjtu.edu.cn`、鉴权 = PAT 首选 / SSO cookie 次选、iCal 导出 = `/profile` 的 `calendar.ics` 字段）。
3. **端点清单替换**：占位 iCal 方案改成本文 §2 实测 `/api/v1/planner/items` 契约。
4. **CLI 设计对齐**：原 `schedule today` / `week` / `next` 改为 `canvas today` / `canvas upcoming --days N` + `canvas whoami` + `canvas setup`；移除 `schedule next`（planner/items 升序取第一条即可，不值得单独命令）。
5. **文件清单对齐** §4.1。
6. **Checkpoint** 表换成本文 §4.4。
7. **新增依赖需预报**：
   - `chrono` 已在（S0）——本地时区换算直接用
   - 不需要新 crate（iCal 路线推迟，不引 `icalendar`）

---

## 6. 调研元数据（可复现）

- 入口：`https://oc.sjtu.edu.cn/`（带已登录 JAccount cookie 的 Chrome → 自动 SSO 落 dashboard）
- 抓包时间：2026-04-24 02:54 UTC
- 浏览器：Chrome/147 via chrome-devtools MCP（page id=2，复用 S3b 抓包会话的 JAccount session）
- Chrome page id=2 自始至终不变（无手工导航到子页）
- 未触发的操作（刻意保留）：
  - `/calendar`（Canvas 内置日历 UI）
  - `/conversations`（站内信）
  - 任何 POST/PUT/DELETE —— MVP 全只读
- 脱敏原则：
  - `x-canvas-user-id` / `_normandy_session` / `_csrf_token` / `log_session_id` **一概替换为占位符**
  - 响应 body 里个人 `id` 也替换
  - 课程名 / 作业标题保留（属讲师发布内容，非 PII）

---

**结论**：S3c Canvas 作业 DDL 可进实装。契约完整、鉴权走 PAT（CLAUDE.md 钦定）、MVP 三只读命令、不需要新依赖。预估工作量 **0.5–0.8 天**（调研 + 现在占了 0.2 天，剩代码 + 3 个 CP）。
