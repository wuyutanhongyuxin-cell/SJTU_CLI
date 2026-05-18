# T4 一卡通 weixin path Fallback — 设计 spec

> 2026-05-18。基线：
> - `2026-05-15-t4-ecard-prerequisites.md`（原始预研）
> - `2026-05-17-t4-update.md`（OAuth2 路径锁定）
> - `2026-05-17-t4-ecard-cas-research.md`（CAS fallback 预研补丁）
> - 2026-05-17 真机调研结论（T19 metadata 完整回填 OQ-CAS-1/2/3/5/6）

**Goal**: 在 OAuth2 path（`api.sjtu.edu.cn` + client_id 阻塞中）之外，新增 **weixin path**：HTML scrape `weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecard*.php` 借用网信中心 client_id 透明完成 OAuth2 token 兑换。CLI 视角等同 cookie+CAS path（同 elec/jwc）。

**Architecture**: 双轨保留 + default weixin 优先。`--via {oauth2, weixin, auto}` flag；`auto` 默认走 weixin（client_id 短期内拿不到），未来 OAuth2 client 审批到位手工改 default。

**Tech Stack**: 复用现有 `scraper`（html5ever）+ `reqwest` cookie jar + `with_cas_refresh`（T8 已实现）+ `rust_decimal::Decimal` + `crate::util::decimal`（T1 已从 elec 抽到 util，顶层 `serialize`/`deserialize`）。

---

## 0. 决策记录（已用户拍板）

| 项 | 锁定 |
|---|---|
| **架构** | 双轨：OAuth2 (`api.sjtu.edu.cn`) 备用 + **weixin** (`weixin.sjtu.edu.cn`) 主用 |
| **fallback 命名** | `weixin path`（淮准实际：HTML scrape，非独立 CAS） |
| **CLI flag** | `--via {oauth2, weixin, auto}`，`auto` 默认 = **weixin 优先** |
| **OAuth2 代码** | keep 不删（src/apps/card/api.rs + oauth_dev/）；未来 client_id 到位后改 default |
| **fallback 触发** | `auto` 模式下：直接走 weixin；`--via oauth2` 用户显式选 OAuth2，错误透传 |
| **金额** | 全 `rust_decimal::Decimal`，禁 f32/f64 |
| **写端点** | 永久红线 — 挂失/解挂/改密码/改照片/拾卡/银行转账等永不实装 |

---

## 1. 入口契约（OQ-CAS-1 回填）

### 1.1 鉴权链（真机抓取）

```
GET https://card.sjtu.edu.cn/                                  [302]
GET https://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecard.php   [200]   # \主页 / 已 jaccount session 时
GET https://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecardbalance.php  [302]   # \未 OAuth2 token
GET https://jaccount.sjtu.edu.cn/oauth2/authorize?
        client_id=janicweixin20150709
        &redirect_uri=http://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecardbalance.php
        &response_type=code
        &scope=profile+connect_wechat+card_info+card_transactions+write_card_info
        &state=4                                                [302]   # \已 jaccount session 时透明跳回
GET https://jaccount.sjtu.edu.cn/jaccount/jalogin?...           [302]
GET https://jaccount.sjtu.edu.cn/oauth2/authorize?context=...&state=4&jatkt=...  [302]
GET http://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecardbalance.php?code=XXX&state=4  [307]
GET https://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecardbalance.php?code=XXX&state=4  [200]   # HTML
```

### 1.2 关键事实

- **client_id**: `janicweixin20150709`（公开，网信中心持有；CLI 不需要申请）
- **scope**: `profile connect_wechat card_info card_transactions write_card_info`（CLI 仅消费 card_info + card_transactions 对应字段，写 scope 红线不点）
- **redirect_uri**: 后端固定 `http://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/<page>.php`
- **CLI 视角**：只需 jaccount cookie（HttpOnly，自动跨 `*.sjtu.edu.cn` 子域共享）；后端 PHP 自动完成 OAuth2 token 兑换 + 调 api.sjtu.edu.cn + 渲染 HTML

---

## 2. Endpoint 契约（OQ-CAS-2/3 回填）

### 2.1 余额查询

```
GET https://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecardbalance.php
Headers: Cookie=<jaccount session>
Response: 200 text/html
```

**HTML 字段**：

| 字段 | 类型 | 形态 | spec note |
|---|---|---|---|
| 校园卡余额 | `Decimal` | `"3.88 元"`，需切 `" 元"` 后转 Decimal | 复用 `util::decimal_str_or_num` 不适用（这是 text 不是 JSON 浮点），手写 `parse_money_zh(&str) -> Decimal` |
| 姓名 | `String` | PII，**CLI 不展示**，spec 阶段建模留字段但 Envelope serialize 时 `#[serde(skip_serializing)]` 或 redact `"***"` |
| 卡账号 | `String` | 6 位数字字符串 |
| 过渡余额 | `Decimal` | `"0 元"` |
| 绑定银行卡 | `Option<String>` | 空 → `None` |
| 挂失状态 | `enum CardLostStatus` | `"正常"` / `"挂失"`；未观察其它值 |
| 冻结状态 | `enum CardFreezeStatus` | `"正常"` / `"冻结"`；未观察其它值 |

### 2.2 流水查询

```
GET https://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecardbill.php
GET https://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecardbill.php?startdate=YYYY-MM-DD&enddate=YYYY-MM-DD  (TBC OQ-CAS-3.1)
Headers: Cookie=<jaccount session>
Response: 200 text/html
```

**默认行为**（无参数）：最近 30 天

**HTML 表格字段（每条记录）**：

| 列 | 类型 | 示例 | spec note |
|---|---|---|---|
| `datetime` | `chrono::NaiveDateTime` | `"2026-05-17 00:41:00"` | format `"%Y-%m-%d %H:%M:%S"` |
| `merchant_short` | `String` | `"六期水控"` / `"闵一内档"` / `"银行转账"` | 部门代码或简称 |
| `merchant_full` | `String` | `"六期水控"` / `"闵行一餐淮扬快餐"` | 详细店名；可能 = `merchant_short` |
| `amount` | `Decimal` | `-0.8`（消费负） / `20`（充值正） | 字符串无单位 |
| `card_balance` | `Decimal` | `3.88` | 交易后卡余额 |

**汇总（页底 footer，optional）**：

| 字段 | 类型 |
|---|---|
| `topup_total` | `Decimal`（如 `"20 元"`） |
| `spend_total` | `Decimal`（如 `"-33.2 元"`） |

### 2.3 时间格式归一

CAS path 拿到 `chrono::NaiveDateTime`（无时区），OAuth2 path 拿到 ms timestamp。Models 层归一为 `chrono::DateTime<chrono::FixedOffset>` 中国时区 `+08:00`，serialize 为 ISO8601 字符串。

---

## 3. CLI 接口

### 3.1 `--via` flag

新增到 `sjtu card balance` / `sjtu card history` 两个子命令：

```bash
sjtu card balance [--via auto|oauth2|weixin]   # default: auto
sjtu card history [--days N] [--via auto|oauth2|weixin]
```

clap derive：

```rust
#[derive(clap::ValueEnum, Clone, Debug, Default)]
pub enum CardVia {
    #[default]
    Auto,
    Oauth2,
    Weixin,
}
```

### 3.2 路径选择器（`auto` 行为）

```text
auto:
  ├─ check ~/.sjtu-cli/sub_sessions/card_oauth.json (OAuth2 token)
  │     ├─ exists & valid → 走 OAuth2 path  (未来 client_id 拿到后主路)
  │     └─ missing / invalid → 走 weixin path  (当前默认)
oauth2: 强制 OAuth2，任何错误透传
weixin: 强制 weixin（用户显式选 / 仅校园网内）
```

⚠️ Note: 这是 default 行为；spec 阶段写成 `pub fn select_via(via_flag: CardVia, has_oauth_token: bool) -> ResolvedVia` 单元可测。

### 3.3 Envelope `meta.via`

所有 card 子命令 envelope 加：

```yaml
meta:
  via: "oauth2" | "weixin"        # 实际走的路径，便于 Agent / 用户感知
  source_hint: "card.sjtu.edu.cn" | "api.sjtu.edu.cn"  # 数据源域，debug 用
```

不放 `data` 内层，避免污染数据 schema。

**前置依赖**：`src/output.rs::Envelope` 当前**无** `meta` 字段（仅 `ok / schema_version / data / error`）。plan 早期任务必须扩展：

```rust
// src/output.rs
#[derive(Debug, Clone, Serialize)]
pub struct EnvelopeMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Envelope<T: Serialize> {
    pub ok: bool,
    pub schema_version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<EnvelopeError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<EnvelopeMeta>,    // NEW
}
```

后向兼容：现有子命令构造 envelope 时 `meta: None`，`skip_serializing_if` 保证 JSON 输出不出现 `meta` 键 — 现有 Agent 解析不破坏。SCHEMA.md 同步加章节描述 meta 字段。

---

## 4. 文件骨架

```text
src/apps/card/
├── mod.rs                   # 复用，加 weixin path 路由
├── api.rs                   # OAuth2 path（已有，keep）
├── http.rs                  # OAuth2 HTTP（已有，keep）
├── throttle.rs              # 已有，可共享
├── models.rs                # 已有单文件（T9 建），新字段 #[serde(default)] 容差扩展
├── tests_parse.rs           # 已有，OAuth2 path 解析测试
├── oauth_dev/               # OAuth2（已有，keep；位于 src/auth/oauth2_dev/）
├── via.rs                   # NEW — CardVia enum + select_via
├── weixin/                  # NEW
│   ├── mod.rs               # 入口 fetch_balance / fetch_history
│   ├── client.rs            # reqwest Client w/ cookie jar；含 cookie_to_set_str helper（拼 name=value; Domain=; Path=; 字符串，因为 Cookie struct 是纯数据无方法）
│   ├── balance_parse.rs     # ecardbalance.php HTML 解析
│   ├── history_parse.rs     # ecardbill.php HTML 解析
│   ├── money.rs             # parse_money_zh("3.88 元") -> Decimal
│   └── tests.rs             # mockito 单测（固定 HTML fixture）

src/commands/card/           # 已有 4 文件：mod.rs / data.rs / handlers.rs / refresh_helper.rs
├── mod.rs                   # 加 --via dispatch
├── handlers.rs              # 加 select_via 路由
├── data.rs                  # 已有，复用
└── refresh_helper.rs        # 已有，复用（OAuth2 token refresh，weixin 不走此路径）

src/cli/card.rs              # 已有，加 --via flag
```

新增文件总行数估算：~600（10-12 个文件，每个 < 100 行）— 远低于 200 行/文件限。

---

## 5. 鉴权层：复用主 session jaccount cookie

### 5.1 cookie source

CLI 已有 `sjtu login` QR 扫码登录拿主 session cookie（`~/.sjtu-cli/session.json`）。该 cookie domain `*.sjtu.edu.cn`，weixin.sjtu.edu.cn 子域**自动共享**。

### 5.2 reqwest client 构造

```rust
// src/apps/card/weixin/client.rs
// 注：Cookie struct 是纯数据（src/cookies/mod.rs:24-33），无 serialize 方法，
// 这里自写 helper 把 Cookie → Set-Cookie 字符串供 jar.add_cookie_str 消费。
fn cookie_to_set_str(c: &crate::cookies::Cookie) -> String {
    let mut s = format!("{}={}", c.name, c.value);
    if let Some(d) = &c.domain { s.push_str(&format!("; Domain={d}")); }
    if let Some(p) = &c.path   { s.push_str(&format!("; Path={p}")); }
    // expires 故意不拼：jar 内部读 cookie 不在乎过期时间，stale 由 SubSessionStale 信号驱动
    s
}

pub(super) fn build_weixin_client(main_session: &Session) -> Result<Client> {
    let jar = std::sync::Arc::new(reqwest::cookie::Jar::default());
    let url = url::Url::parse("https://weixin.sjtu.edu.cn/")?;
    for cookie in &main_session.cookies {
        jar.add_cookie_str(&cookie_to_set_str(cookie), &url);
    }
    Client::builder()
        .cookie_provider(jar.clone())
        .redirect(Policy::limited(10))   // CAS 链最多 ~8 跳
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .gzip(true)
        .user_agent(UA)
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 weixin client: {e}")).into())
}
```

### 5.3 with_cas_refresh 包装

复用 `src/auth/cas/retry::with_cas_refresh` —— stale 时（`SubSessionStale("card_weixin")`）触发 `clear_sub_session + cas_login refresh`。

跟 elec/jwc 同款，**不需要新写 refresh helper**。

---

## 6. Models

### 6.1 共享 struct（`models/card_record.rs`）

字段 `#[serde(default)]` 容差，两路兼容：

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Transaction {
    pub datetime: DateTime<FixedOffset>,
    pub system: Option<String>,           // OAuth2 有 / weixin merchant_short
    pub merchant: Option<String>,          // OAuth2 有 / weixin merchant_full
    pub description: Option<String>,       // OAuth2 有 / weixin 无
    pub merchant_no: Option<String>,       // OAuth2 有 / weixin 无
    #[serde(with = "crate::util::decimal")]
    pub amount: Decimal,
    #[serde(with = "crate::util::decimal")]
    pub card_balance: Decimal,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CardInfo {
    pub card_no: String,
    #[serde(with = "crate::util::decimal")]
    pub balance: Decimal,
    #[serde(default, with = "crate::util::decimal_opt")]
    pub transition_balance: Option<Decimal>,   // weixin 独有；None = 未观察到，Some(0) = 真实 0 元
    pub lost_status: Option<CardLostStatus>,   // weixin 独有
    pub freeze_status: Option<CardFreezeStatus>,
    // 姓名 不加到这里，redact 于 parse
}
```

**前置依赖**：`crate::util::decimal_opt` 当前不存在，plan 第一个 T 任务必须先建 `src/util/decimal_opt.rs`（约 30 行，包装 `util::decimal` 的 `serialize`/`deserialize` 处理 `Option<Decimal>`：`None` ↔ JSON `null`，`Some(d)` ↔ JSON 字符串）。

### 6.2 HTML 解析

使用 `scraper` 已有依赖。固定 CSS selector，HTML 改版时 graceful degrade：

```rust
// src/apps/card/weixin/balance_parse.rs
pub(super) fn parse_balance(html: &str) -> Result<CardInfo> {
    let doc = scraper::Html::parse_document(html);
    // 根据真机 snapshot 结构：root > <table> | <div> 等
    // 字段提取 with fallback selector 序列（防改版）
    // ...
}
```

每个字段提取失败 → `tracing::warn` + `field=None`，整体函数仍返成功。

---

## 7. 写端点（永久红线）

| 端点 | weixin path 立场 |
|---|---|
| `ecardlost.php` 挂失/解挂 | ✗ 不实装 |
| `ecardpassword.php` 改密码（PII） | ✗ 不实装 |
| 充值 / 改照片 / 拾卡 / 圈存 / 转账 / 退款 | ✗ 不实装 |
| 个人信息维护（手机 / 地址 / 银行卡） | ✗ 不实装 |

`powerbalance.php` 电费查询是 elec 子系统的事，weixin path 不涉及。

---

## 8. Open Questions（plan/CP 阶段补）

- **OQ-WX-1**：`ecardbill.php` time range query 参数确切 key 名（`startdate`/`enddate` 还是其它）— 真机 click "查询流水"抓 URL 或 mockito 多试。
- **OQ-WX-2**：session stale 形态 — 30 分钟 session 过期后端是 302 → jaccount jalogin 还是其它？spec 假设同 elec/jwc，CP 验证。
- **OQ-WX-3**：HTML 解析 selector 稳定性 — 不带 class/id 的 `<div>` 嵌套需要按文本 anchor，未来 SJTU 改版风险评估。

---

## 9. Out-of-scope（本 spec 不做）

- OAuth2 path 改动：保持原状（一行不动）
- elec/services/jwbmessage 接入新 CAS retry 层（之前 follow-up，不在 T4 范围）
- Phase 2 多卡支持（用户实际只有 1 张主卡）
- HTML response 内联 PII redact 之外的脱敏（姓名以外的 PII）

---

## 10. 测试策略

- **单元测试**：mockito + 固定 HTML fixture（脱敏，从真机调研 HTML 节略）
- **集成测试**：`#[ignore]` + 真机 CP（`cargo test -- --ignored`）
- **CI**：仅 mockito 单测，绝不真打 SJTU 服务器

---

## 11. 下一步

1. ✅ spec 用户过审（本文档）
2. ⏳ 写 plan `docs/superpowers/plans/2026-05-18-t4-ecard-weixin-fallback.md`（TDD bite-sized steps）
3. ⏳ subagent-driven execute（fresh subagent per task + spec/code-quality two-stage review）
4. ⏳ 真机 CP（解 OQ-WX-1/2/3）
