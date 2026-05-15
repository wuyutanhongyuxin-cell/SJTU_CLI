# T4 一卡通余额 + 消费明细 — 实装前 Unknowns 清单

> 2026-05-15 预研报告。不含真实 fixture；全部端点需真机验证。

---

## 1. 现有 CAS 子系统先例分析

### 标准文件骨架（参考 `apps/elec/`）

```
src/apps/card/
├── mod.rs          — pub use + 模块声明（~25 行）
├── api.rs          — Client struct + connect() + 只读端点（~90 行）
├── http.rs         — build_http_client + fetch_json + 重试（~130 行）
├── models.rs       — 响应 struct + decimal_str_or_num（~120 行）
├── throttle.rs     — Throttle 节流（复制 elec，改 MIN_INTERVAL，~40 行）
└── tests_parse.rs  — 单元测试（~60 行）

src/commands/card/
├── mod.rs          — pub use（~10 行）
├── data.rs         — *Data struct（Serialize，~60 行）
└── handlers.rs     — cmd_balance / cmd_history（~80 行）
```

### handlers.rs CAS retry 样板（当前形态，T8 后会再改一次）

- `Client::connect()` 内调 `cas_login(name, LOGIN_URL)`
- 返回 `CasResult { session, from_cache, elapsed_ms, final_url }`
- `build_http_client(&r.session)` 注入 cookie 构造 `reqwest::Client`
- T8 实装后，`connect()` 会改为持有 retry helper；card 模块届时跟进，**不要现在自己造**

### Envelope schema 跨子系统约束

- 所有命令层输出 `Envelope<T>` (`ok / schema_version / data / error`)
- `schema_version = "1"`（`output.rs` 常量）
- 金额字段：`rust_decimal::Decimal`，序列化为字符串（`"284.25"` 而非 `284.25`）
- 身份字段（姓名/学号）：命令层 `*Data` struct 默认不放，仅 `--with-identity` 时暴露

---

## 2. 一卡通后端 Unknowns（核心）

### 入口域名候选

| 候选域 | 来源 | 校外可达？ | 验证状态 |
|---|---|---|---|
| `ecard.sjtu.edu.cn` | 官网 + 搜索结果 | **否**（跳 restrict.sjtu.edu.cn，已知） | 需校网真机 |
| `card.sjtu.edu.cn` | 搜索结果（`https://card.sjtu.edu.cn/`） | 未知 | 需真机访问 |
| `api.sjtu.edu.cn/v1/me/card/*` | 官方开发者平台 + dyweb/beancount-sjtu 佐证 | **是**（OAuth2 token，校外可用） | 需申请 clientId |

### 进入方式：三条路径各自的验证需求

**路径 A：`ecard.sjtu.edu.cn` + CAS session（同 elec 模式）**
- 已知：校外跳 `restrict.sjtu.edu.cn`，off-campus 不可达（S3e CP-E0.1 确认）
- 需验证：校园网内 CAS 链是否等同 `cas_login("ecard", "https://ecard.sjtu.edu.cn/")` 即可
- 需验证：session cookie 名称（是否同 elec 的 `JSESSIONID` + `keepalive`）
- 需验证：是否存在 REST JSON API（`/api/*`）还是只有老式 Struts `.action` 页面（`homeLogin.action`）

**路径 B：`card.sjtu.edu.cn`（新域名，功能未知）**
- 来源：搜索返回 `https://card.sjtu.edu.cn/`，内容未知
- 需验证：是否与 ecard 同一套后端、还是独立系统
- 需验证：认证方式（jAccount CAS / OAuth2 / 其他）

**路径 C：`api.sjtu.edu.cn/v1/me/card/*` + OAuth2（开发者平台官方 API）**
- 已知端点（官方文档 + dyweb/beancount-sjtu 实证）：
  - `GET /v1/me/card/user` — 卡用户信息（scope: `card_info` 或 `manage_card`）
  - `GET /v1/me/card/transactions?cardNo=&beginDate=<ms>` — 消费流水（scope: `card_transactions`）
  - `GET /v1/me/card/photo` — 卡照片
- 认证：Authorization Code OAuth2，`access_token` 参数或 Bearer 头
- **关键约束**：需向学校申请 `clientId` + 密钥（`my.sjtu.edu.cn` → 信息服务 → jAccount 接口申请），**3 个工作日审批**
- 需验证：`balance` 字段名、字段类型（string/number）、Decimal 精度
- 需验证：`transactions` 时间范围参数格式（毫秒时间戳 vs YYYY-MM-DD）
- 需验证：`transactions` 翻页参数（`page` / `offset` / cursor？）

### 金额类型硬约束

- CLAUDE.md 明确：**所有金额一律 `rust_decimal::Decimal`，禁 f64**
- 参考 elec：服务端混合类型（string / number）需 `decimal_str_or_num` 自定义 ser/de
- 预判：`api.sjtu.edu.cn` 返回的 `cardBalance` 可能是 number（如 `12.50`），需 `visit_f64` 兜底

### 翻页 / 时间范围参数 Unknowns

- `beginDate` 参数格式：毫秒时间戳（beancount-sjtu 示例中用 `1598889600000`）vs YYYY-MM-DD，**需真机看 request**
- `endDate` 参数：是否存在？不传时服务端默认截止时间？
- 翻页：是否有 `page` / `size` 参数？最大单次条数？
- `cardNo` 参数：是否可以省略（自动取当前用户）？

---

## 3. WebSearch 轻调研结果

**找到的公开资料（3 个有效来源）：**

1. **官方开发者平台** `https://developer.sjtu.edu.cn/api/card.html`
   - 确认存在 `GET /v1/me/card/user`、`GET /v1/me/card/transactions`
   - Base URL: `api.sjtu.edu.cn`，需 OAuth2 Authorization Code flow + `card_transactions` scope

2. **dyweb/beancount-sjtu** `https://github.com/dyweb/beancount-sjtu`
   - 实证：`GET https://api.sjtu.edu.cn/v1/me/card/transactions?access_token=<token>&cardNo=<no>&beginDate=<ms_ts>`
   - 证明该端点**校外可访问**（不依赖校园网）
   - Go 实现，可参考字段映射

3. **ecard.sjtu.edu.cn 老系统** `http://ecard.sjtu.edu.cn/homeLogin.action`
   - 老式 Struts `.action` 路由，非 REST JSON
   - 需校园网；登录用"学工号 + 查询密码"（独立于 jAccount？）
   - CLI 优先级低于路径 C

**无 SJTU 专属 GitHub 爬虫项目找到**（只有 scu 等他校类似项目，不可直接复用）。

---

## 4. 推荐真机调研步骤（给主对话用户）

用 chrome-devtools 在**校园网**环境下按以下顺序操作：

1. **访问 `https://card.sjtu.edu.cn/`**：截图，看是否与 `ecard.sjtu.edu.cn` 同内容 or 独立系统；记录 Network 里是否有 JSON API 请求。

2. **访问 `http://ecard.sjtu.edu.cn/homeLogin.action`**：看登录页面形态；用 jAccount 扫码能否直接登入（还是要单独的"查询密码"）；记录 Set-Cookie 里的 cookie 名称。

3. **登录成功后访问余额页**：在 Network 面板里找余额相关 XHR；记录：URL、请求头（特别是 Cookie/Authorization）、响应 JSON 结构和字段名（尤其金额是 string 还是number）。

4. **访问消费明细页**：点"消费查询" / "流水查询"；记录端点 URL、时间范围参数格式（看 query string）；翻页时记录翻页参数。

5. **访问 `https://developer.sjtu.edu.cn/api/card.html`**：截图完整文档，确认 `/v1/me/card/transactions` 的参数列表（`beginDate`/`endDate`/`page`/`size`）。

6. **在校网内尝试 `curl https://api.sjtu.edu.cn/v1/me/card/user?access_token=INVALID`**：看返回错误格式（确认端点可达 + 错误结构）。

7. **检查是否需要申请 clientId**：访问 `my.sjtu.edu.cn` → 信息服务 → jAccount 接口申请，看申请表单里是否有 `card_transactions` scope 选项。

8. **对比两条路径**：如果 `ecard` 老系统有 JSON API 且不需 OAuth2 申请，成本更低；如果只有 `api.sjtu.edu.cn` 路径，记录 OAuth2 流程是否支持"已有 JAAuthCookie 免重新授权"。

---

## 5. 实装前血路图

### 阻塞项（必须先解决）

- **[BLOCK-1] 入口确认**：路径 A（ecard CAS）vs 路径 C（api.sjtu OAuth2），二选一后才能定 `apps/card/` 骨架
  - 路径 A：无需申请 clientId，但校外不可达（用户使用受限）
  - 路径 C：校外可达，但 OAuth2 需申请审批（3 工作日），且现有 QR login 流程与 OAuth2 Authorization Code 不同，需额外适配
- **[BLOCK-2] 余额字段名**：不确定是 `balance` / `cardBalance` / `SYLJE` 风格，无法写 models.rs
- **[BLOCK-3] transactions 时间参数格式**：毫秒 ts vs YYYY-MM-DD，影响 CLI `--days N` 参数转换逻辑

### 不阻塞项（可延后）

- 翻页上限（`--limit` 参数默认值可先写 100，真机验后调整）
- `cardNo` 是否必传（先实现"自动取当前用户"路径）
- 充值历史（recharge history）端点（独立于消费流水，phase-2 内再拆）
- Table 渲染格式（CLI 列宽可后期调）

### 与 CAS retry T8 的耦合关系

- T8 目标：为所有 CAS 子系统客户端加 `with_token_refresh` 包，检测 session stale 后自动重做 CAS
- **card 模块应等待 T8 完成后再实装**，或先用当前 `cas_login` 直接调用，T8 后跟进一次改造
- 若走路径 C（OAuth2），则不走 `cas_login`，而是独立的 OAuth2 token 刷新逻辑——与 T8 解耦，但需新增 `src/auth/oauth2.rs` helper（需用户批准新模块）

---

## 6. 200 行限预估

| 文件 | 预估行数 | 说明 |
|---|---|---|
| `apps/card/mod.rs` | ~25 | pub use + 模块声明 |
| `apps/card/api.rs` | ~90 | Client + connect + balance + transactions |
| `apps/card/http.rs` | ~130 | build_http_client + fetch_json（复用 elec 模式） |
| `apps/card/models.rs` | ~110 | 3 个 struct + decimal_str_or_num |
| `apps/card/throttle.rs` | ~40 | 复制 elec/throttle.rs，300ms 间隔 |
| `apps/card/tests_parse.rs` | ~60 | 2-3 个 fixture 单测 |
| `commands/card/mod.rs` | ~10 | pub use |
| `commands/card/data.rs` | ~55 | BalanceData + HistoryData |
| `commands/card/handlers.rs` | ~75 | cmd_balance + cmd_history |
| **合计** | **~595** | 全部在 200 行/文件 限制内 |

若走路径 C（OAuth2），额外新增 `src/auth/oauth2.rs`（~120 行），需用户批准。

---

> 本报告不含任何真实 cookie / 学号 / 余额数据。
