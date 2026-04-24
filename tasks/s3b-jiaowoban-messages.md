# S3b 交我办消息中心 —— 调研结果

> 调研日期：2026-04-24（浏览器实测 `my.sjtu.edu.cn`，账号脱敏）
> 方法：chrome-devtools MCP 抓 XHR + 反查 JS bundle 里的 URL 字符串
> 状态：✅ 契约全部锁定，可直接进实装（CLI 只读 + 一个"全部已读"写）

---

## 1. 链路归类

`plan-next.md §⚪ S3b` 给的三选一 —— **命中路径 2：挂在 `my.sjtu.edu.cn` 下，主 session 直接用**。

| 证据 | 观察值 |
|---|---|
| SPA 路由 | `https://my.sjtu.edu.cn/ui/message/` + `/ui/message/detail?...`（同域，前端路由） |
| XHR 域 | 全部 `my.sjtu.edu.cn/api/*`，无跨域 |
| 鉴权 cookie | `JSESSIONID`（关键）+ `keepalive`（一次性续期值）+ `PORTAL_LOCALE`（只影响语言） |
| 关键请求头 | `X-Requested-With: XMLHttpRequest` + `Accept: application/json` |
| CSRF | **无** —— 所有写端点只靠 cookie + X-Requested-With |
| Bearer / OAuth2 | 无 |

### 对实装的意义

- **不需要写新的 login flow**。沿用 S1 `qr_login::login_with_chrome()` 抓的主 session，cookies.json 里 `JSESSIONID` 就能直接打。
- 连 `oauth2_login("shuiyuan", ...)` 那套重定向链都不用，比 S3a 简单得多。
- 但 `JSESSIONID` 是短寿命 session cookie，过期后需要走一次 CAS 刷新 —— 建议 **复用 S2 `cas_login` 作为 401 兜底**（不是入口）。具体：第一次直接用主 session 打 API，401 就走一次 `cas_login("my.sjtu.edu.cn/ui/app/")` 刷 cookie 再重试。
- `keepalive` cookie 每次响应都会 `Set-Cookie` 刷新，reqwest 的 cookie store 自动处理，不用手工维护。

---

## 2. 端点契约

### 2.1 未读数 `GET /api/jwbmessage/unreadNum`

```
GET https://my.sjtu.edu.cn/api/jwbmessage/unreadNum
Headers:
  X-Requested-With: XMLHttpRequest
  Accept: application/json
Cookie: JSESSIONID=...

→ 200 application/json
{"total": 108, "errno": 0, "entities": [], "error": null}
```

### 2.2 分组列表 `GET /api/jwbmessage/group`

```
GET /api/jwbmessage/group?key=&page=0&pageSize=10&read=false

Query:
  key       搜索关键词（空=全部）
  page      从 0 开始
  pageSize  默认 10
  read      false = 只看有未读的分组？true = 看全部？（默认前端传 false；回应里 15 个 entities）
            —— 实测 read=false 返回 15 个，包含已读分组（与名字直觉相反，可能是"read=是否包含已读"）

→ 200 application/json
{
  "success": true, "errno": 0, "message": "success",
  "total": 15,
  "entities": [
    {
      "groupId":   "AGM4V5mqXn2PDwznSErB",   // 分组标识（App key）
      "groupName": "Canvas日历通知",
      "unreadNum": 4,
      "groupDescription": "…",               // 该分组最新一条的摘要
      "isGroup":   false,                     // 是否是"合并分组"（很少见 true）
      "isRead":    true,                      // 与 unreadNum>0 不矛盾 —— 语义待定
      "icon":      "https://api.sjtu.edu.cn/v1/file/<uuid>" | null,
      "createTime":"2026-04-24 09:00:46"     // 最新消息时间
    },
    ...
  ],
  "entity": null
}
```

**坑点**：
- `isRead` 和 `unreadNum` 同时出现且矛盾（`unreadNum:4, isRead:true`）。建议 **以 `unreadNum` 为准**，`isRead` 字段忽略。
- `groupId` 对不同 App 形态不一：有 UUID（`EB6A023D-...`）、有短字符串（`gD7xfTi3zhrAt94Njg7o`）、有纯数字（`50500`）、有 `jaform101110` —— 都按字符串处理。

### 2.3 组内消息列表 `GET /api/jwbmessage/messagelist`

```
GET /api/jwbmessage/messagelist?page=0&pageSize=10&key=&groupId=<id>&isGroup=<bool>&read=<bool>

Query:
  groupId    必传，来自 2.2 的 entity.groupId
  isGroup    必传，来自 2.2 的 entity.isGroup
  read       true 或 false（实测 true 返回全部含已读 + 未读；前端在详情页传 true）
  page/pageSize/key  同 group

→ 200 application/json
{
  "success": true, "errno": 0, "message": "success",
  "total": 23,
  "entities": [
    {
      "messageId":      "f88610ba-d463-4ffc-8e8a-4370177b53e5",
      "type":           "basic",
      "title":          null,
      "description":    "班车预约成功",           // 列表摘要
      "readTime":       null,                      // 已读时间；null = 未读
      "read":           false,
      "expireTime":     null,
      "notificationId": "75033ea2-318f-11f1-8860-fa163ecd40a6",
      "createTime":     "2026-04-06 16:06:04",
      "pushTitle":      "班车预约成功",
      "pushContent":    "…完整正文…",             // 详情正文（纯文本，含 \n）
      "authClient": {                              // 发送方 App 元数据
        "name":        "学生预约乘车",
        "apiKey":      "gD7xfTi3zhrAt94Njg7o",
        "description": "校区间通勤班车预约管理系统。",
        "iconUrl":     "https://api.sjtu.edu.cn/v1/file/<uuid>"
      },
      "picture":  null,
      "urlList":  null,
      "context":  [ { "key": "内容", "value": "…" } ]    // 结构化详情，key 与 value 成对
    },
    ...
  ],
  "entity": null
}
```

**关键发现：无独立详情端点**。单条正文已经在 `messagelist.entities[].pushContent`（纯文本）/ `context[]`（结构化）里。CLI 的 `show` 命令直接从 list 结果里挑出该 messageId 渲染即可，无需再发请求。

### 2.4 批量全部已读 `POST /api/jwbmessage/message/readall`

来源：JS bundle 反查。

```js
// 前端调用源码（整理自 chunk-xxxxxx.js）
return Object(c['a'])("/api/jwbmessage/message/readall", {
  method: "post",
  data: {},
  headers: { "Content-Type": "application/json" }
})
```

```
POST /api/jwbmessage/message/readall
Headers:
  Content-Type: application/json
  X-Requested-With: XMLHttpRequest
  Accept: application/json
Body: {}

→ 200（未实测，不点"全部已读"避免破坏账号状态）
```

**警告**：
- 这个端点是"把所有未读标记为已读"，**不是**按 groupId 批量。**水源的 `--yes` 二次确认必须强制**。
- 没有找到"按分组标记已读"或"按单条标记已读"的端点 —— 说明水源这个模块**只有 all-or-nothing**。

### 2.5 隐式副作用：`messagelist` 会自动标记已读

**实测现象**：进入 `学生预约乘车` 分组前 `unreadNum=110`，`GET messagelist?groupId=gD7xfTi3zhrAt94Njg7o&...` 一次后立即变 `108`（减 2，正好对应该组 2 条未读）。

**解读**：`GET messagelist` **不是纯读** —— 后端把"用户看了这个组的列表"视为已读信号，静默标记该组下所有未读。

**对 CLI 的影响**：
- 若 CLI 只想"瞄一眼有没有新消息"而**不**改状态，**只能打 `unreadNum` + `group`**，**绝对不能**打 `messagelist`。
- 如果 CLI 想要"只看未读"，要么：
  - (a) 先 `group` + 筛选 `unreadNum > 0` 的分组 —— 只告诉用户"哪些 App 有新消息"，不触发已读；
  - (b) 或打 `messagelist` 接受副作用（类似邮件客户端的行为）。
- **建议**：CLI 提供两档 —— `sjtu messages`（只 `group`，不标已读）/ `sjtu messages show <group>`（打 messagelist，承认会标已读）。

### 2.6 `operationDisable`（跳过 —— MVP 不需要）

POST 端点（GET 返 405），body 是某 JSON 对象，语义上像"屏蔽某 App 的推送"。MVP 不做。留作 S3b 后续扩展：`sjtu messages mute <group>`。

---

## 3. 其他顺带捕获（可能反哺 S3c）

抓页面加载时还顺手看到这些端点（未深挖）：

| 端点 | 用途 | 可能对接子系统 |
|---|---|---|
| `GET /api/account` | 当前用户完整信息（姓名/学号/学院/班级/身份/邮箱/手机） | **S0 whoami 升级：比当前只抓 username 多 10 个字段** |
| `GET /api/calendar/today` | 今日日程 | **S3c 日程 → 直接命中** |
| `GET /api/task/me/processes/todo` | 流程待办 | S3b+ 扩展 |
| `GET /api/task/me/processes/cc?limit=1&unread=true` | 流程抄送 | S3b+ 扩展 |
| `GET /api/task/me/recentlyApp?limit=10` | 最近使用 App | 门户类 |
| `GET /api/task/me/apps` | 我的 App 列表 | 门户类 |
| `GET /api/resource/getTotalNotify` | 聚合通知数 | 顶部徽标用 |

**重要收获**：S3c（日程）调研可以**省一半** —— `GET /api/calendar/today` 就在这里，同一条 cookie 链打。先把 S3b 做了顺带把 S3c 也摸了。

---

## 4. 实装建议（供进 S3b-write 阶段参考，不是强制）

### 4.1 文件清单

沿用 `plan-next.md §⚪ S3b` 的骨架，但拆得更细以守 200 行硬限：

```
src/apps/jwbmessage/
├── mod.rs                   # Client struct + connect() + 转发
├── api.rs                   # 读端点（unreadNum / group / messagelist）
├── api_write.rs             # readall（唯一写端点）
├── models.rs                # Group / Message / AuthClient 等 struct
├── tests_parse.rs           # 解析测试
└── tests_write.rs           # mockito readall 测试
src/commands/jwbmessage/
├── mod.rs
├── data.rs                  # Envelope Data
├── handlers_read.rs         # list / show
└── handlers_write.rs        # read-all（强制 --yes）
src/cli/jwbmessage.rs        # clap Subcommand
```

### 4.2 CLI 命令（修正版，偏离 plan-next.md 的占位设计）

```
sjtu messages                         # = group list（不标已读）
  --unread-only                       # 只显 unreadNum>0 的分组
  --limit N                           # pageSize
  --page N

sjtu messages show <group-name|id>    # = messagelist（会标已读！）
  --limit N                           # 返回 N 条（pageSize）
  --all                               # 连已读一起返（read=true）
  默认只显未读

sjtu messages read-all --yes          # = readall（强制 --yes）
```

**不做的命令**（因为端点不存在）：
- `sjtu messages mark-read <id>` —— 没有单条 mark-read 端点
- `sjtu messages mark-group-read <group>` —— 没有按组端点

### 4.3 鉴权层复用

```rust
// src/apps/jwbmessage/mod.rs (示意)
impl Client {
    pub async fn connect() -> Result<Self> {
        // 1) 读 ~/.sjtu-cli/session.json（S1 抓的主 session）
        let session = crate::cookies::load_main()?;
        // 2) 构造 reqwest Client 注入 cookie
        let http = build_http_client(&session)?;
        // 3) 试打一次 /api/account 看 401 否；401 则走 cas_login 刷新
        match fetch_account(&http).await {
            Ok(_) => Ok(Self { http }),
            Err(e) if is_401(&e) => {
                let session = cas_login(MY_SJTU_URL).await?;
                let http = build_http_client(&session)?;
                Ok(Self { http })
            }
            Err(e) => Err(e),
        }
    }
}
```

### 4.4 Checkpoints（实装时对）

| 编号 | 命令 | 预期 |
|---|---|---|
| CP-M1 | `sjtu messages --unread-only --limit 5 --yaml` | `data.returned ≤ 5`，每条含 `groupId/groupName/unreadNum/groupDescription/createTime` |
| CP-M2 | `sjtu messages show gD7xfTi3zhrAt94Njg7o --limit 3 --yaml` | 返 3 条消息，每条含 `pushTitle/pushContent`；调用后 `sjtu messages --unread-only` 该 group 消失或 unreadNum 归零 |
| CP-M3 | `sjtu messages read-all --yes` | `data.marked: true`；后续 `sjtu messages --unread-only` 返 0 条（高风险，CI 里 skip） |

---

## 5. 对 plan-next.md 的回写清单

调研完成后需要修改 `plan-next.md`：

1. `§⚪ S3b 消息中心 → 端点` 区块的占位 `/msg/list` / `/msg/<id>` / `/msg/<id>/read` **全部替换**为本文件 §2 的实测端点。
2. `§⚪ S3b 消息中心 → CLI 设计` 要改：
   - 移除 `sjtu messages mark-read <id>`（端点不存在）
   - 移除 `sjtu messages mark-all-read --yes` 重命名为 `sjtu messages read-all --yes`
   - 新增注解："`show` 隐含标记已读，不可逆"
3. `§⚪ S3b 消息中心 → 调研项` 清单全部打钩。
4. `§⚪ S3c 日程` 区块提示：`GET /api/calendar/today` 已在本次调研顺带捕获，同 cookie 链，调研量减半。

---

## 6. 调研元数据（可复现）

- 入口 URL：`https://my.sjtu.edu.cn/ui/app/`
- 登录后门户首屏右上角"消息"按钮（`div.menu-item` 内含 `span.count`）点击 → 跳 `/ui/message/`
- 抓包时间：2026-04-24 02:05–02:11 UTC
- 浏览器：Chrome/147 via chrome-devtools MCP
- Chrome page id=1 自始至终不变（SPA 前端路由，整个流程 0 次真正跳转）
- 未触发的操作（刻意保留）：
  - "批量阅读"按钮（屏幕右上角扫把图标）
  - "全部已读" `POST /message/readall`
  - `POST /operationDisable`

---

**结论**：S3b 可进实装。契约完整、鉴权沿用主 session、写端点只有一个（readall，强制 --yes）。预估工作量 **0.8 天**（原估 1–2 天，调研吃掉大头）。
