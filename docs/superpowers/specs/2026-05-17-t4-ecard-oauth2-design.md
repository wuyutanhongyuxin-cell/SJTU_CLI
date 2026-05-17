# T4 一卡通 OAuth2 — Design Spec

> **状态**：Draft → 待用户过审 → 转 writing-plans
> **日期**：2026-05-17
> **预研基线**：`docs/superpowers/research/2026-05-15-t4-ecard-prerequisites.md` + `2026-05-17-t4-update.md`（公网调研补丁）
> **范围**：新增 `apps/card/` 子系统 + `auth/oauth2_dev/` OAuth2 Authorization Code 通道；MVP 仅实装两个只读命令 `sjtu card balance` + `sjtu card history`
> **复用基线**：`apps/elec/`（envelope `{errno,error,total,entities}` + `decimal_str_or_num` ser/de）/ `commands/canvas_video/retry.rs::with_token_refresh`（refresh-on-401 同构 pattern）/ `cookies::{save_sub_session, load_sub_session}` / `output::Envelope<T>`

---

## 1. 背景与现状

### 1.1 路径选择决策

公网调研（2026-05-17，curl 真机）确认三条候选路径的可达性：

| 路径 | 域 | 公网行为 | 决策 |
|---|---|---|---|
| A | `ecard.sjtu.edu.cn` | 302 → `restrict.sjtu.edu.cn` | ✗ 永久绑校园网，不可接受 |
| B | `card.sjtu.edu.cn` | 302 → `weixin.sjtu.edu.cn/.../ecard.php` | ✗ 微信 H5 入口，非 REST API |
| C | `api.sjtu.edu.cn/v1/me/card*` | 200 + `{errno,error,total,entities}` | ✓ **唯一选路** |

路径 C 公网可达，envelope 与 elec/services 完全同款，无需新基础设施。

### 1.2 OAuth2 标准 vs 现 codebase

| 现有模块 | 用途 | 路径 |
|---|---|---|
| `src/auth/qr_login.rs` | jAccount QR 扫码拿 JAAuthCookie | 主 session |
| `src/auth/cas/` | CAS 子系统跳转拿 cookie session | jwc / elec / services / jwbmessage |
| `src/auth/oauth2/` | shuiyuan Discourse 用，**跟 302 链拿 `_t` cookie**，不是标准 OAuth2 | shuiyuan |

**关键澄清**：现 `src/auth/oauth2/` **不是 RFC6749 Authorization Code Grant**——它依赖 jAccount 已登录的主 session cookie 自动通过 jaccount.sjtu.edu.cn 的 OAuth2 授权页（implicit-trust 跟 302 链），落点拿的是 Discourse 自家的 `_t` session cookie，**不拿 access_token**。

T4 必须走**真正的 Authorization Code Grant**：
1. 浏览器跳 `authorize?response_type=code&...` → 用户同意 → 回调拿 `code`
2. 后端 POST `/oauth2/token` 用 code + client_secret 换 `access_token` + `refresh_token`
3. Bearer access_token 调 `api.sjtu.edu.cn/v1/me/card`
4. 1800s 过期前用 refresh_token 续

这与 shuiyuan 路径**语义层完全不同**，必须新模块。

### 1.3 资源约束

- **clientId 申请审批 3 工作日**：阻塞真机端到端验证，但**不阻塞** spec/plan/TDD 代码骨架（已经有完整契约 + mockito 单测可独立跑）
- **CLAUDE.md 硬约束**：金额一律 `rust_decimal::Decimal`，禁 f64；不引入新依赖
- **CLAUDE.md 红线扩展（本 spec 落定）**：一卡通"信息维护"、"挂失/解挂/补卡"、"充值"、"绑定银行卡"等任何按钮 CLI 永久不点

---

## 2. Goals / Non-Goals

### Goals

- **G1 — `sjtu card balance`**：返当前余额、过渡余额、卡号脱敏、状态（lost/frozen）；默认抹银行卡号 + 学号 + 姓名 + 单位，`--with-identity` 才出
- **G2 — `sjtu card history --days N`**：返时间窗口内消费记录（默认 30 天），每条含消费时间/商户/金额/交易后余额；total_kwh 同款累加用 `Decimal`，避免 f64 精度坑
- **G3 — OAuth2 Authorization Code 完整流程**：本地 server `127.0.0.1:45123/callback` 接 code，自动浏览器跳 authorize URL（仿照 QR 登录的 headless_chrome 模式或 `open` crate）；access_token + refresh_token 落 `~/.sjtu-cli/sub_sessions/card_oauth.json` chmod 600
- **G4 — Refresh 透明续期**：`with_token_refresh` helper 包裹所有 `api.sjtu.edu.cn` 调用，401/`errno=10002` 自动用 refresh_token 续 access_token 重试一次，用户无感
- **G5 — `Decimal` 单一来源**：把 `apps/elec/models.rs::decimal_str_or_num` 提升到 `src/util/decimal.rs::decimal_str_or_num`（`pub(crate)` 共享），elec / card 都从同一 helper 走
- **G6 — 测试**：mockito 单测覆盖 OAuth2 code 换 token、token 刷新、card 端点 entity 反序列化、`history --days N` 时间窗算法；真机 4 个 checkpoint 待 clientId 拿到后串
- **G7 — 文件行数硬约束**：每文件 < 200 行；预估 17 个新文件共 ~1100 行（明细见 §4.4）

### Non-Goals

- **NG1 — 写端点全不实装**：充值/挂失/解挂/改密码/补卡/拾卡 5 类一律 CLI 红线，spec 永久排除
- **NG2 — 路径 A（ecard CAS）不做**：永久绑校园网，不再作为兜底方案；将来若用户出差/在校外即不可用，违 G1 设计目标
- **NG3 — 多卡支持只做 1 张**：首次跑 `GET /v1/me/card` 拿 `entities[0].cardNo` 作"主卡"，存入 `card_oauth.json`；多卡用户后续可加 `--card-no <NO>` 参数（phase-2）
- **NG4 — PKCE 不实装（首版）**：官方文档未明确要求；spec 留 Open Question OQ-1，待审批时确认服务端是否拒绝无 `code_challenge` 的 authorize 请求
- **NG5 — 不引入新 crate**：必要 helper（OAuth2 / 本地 HTTP server）用 reqwest + hyper（hyper 已是 reqwest 的内部依赖，无需 Cargo.toml 改动；用 `tokio::net::TcpListener` + 手卷 1 行 HTTP/1.1 callback 解析即可）
- **NG6 — `essential` / `profile` / `privacy` scope 不申请**：CLI 不需要身份信息，最小权限原则
- **NG7 — Phase-2 不在本 spec**：多卡列表 / 充值历史端点 `/v1/me/card/recharge` / 卡照片 / 一卡通 IDC 详细信息查询 全留 phase-2

---

## 3. 关键设计决策

### 3.1 新模块位置

`src/auth/oauth2_dev/`（与 `src/auth/oauth2/` shuiyuan 通道并列）。

**为何不复用现 oauth2/**：见 §1.2，语义层完全不同（跟 302 vs 换 access_token），共享只会引诱出错。

**为何不改 oauth2/ → oauth2_shuiyuan/**：会牵连 shuiyuan import / cli / handlers ~6 处改动，与 T4 无关；YAGNI 推迟到未来真正需要时。

### 3.2 redirect_uri 与本地 callback server

- **redirect_uri**: `http://127.0.0.1:45123/callback`（固定端口）
- **本地 server**: `tokio::net::TcpListener::bind("127.0.0.1:45123")` + 手工 1 个 HTTP request 解析 + 200 OK 返回静态 HTML "授权成功，可关闭窗口"
- **端口冲突处理**: bind 失败时 envelope `error.code = "port_in_use"`，提示用户关掉占用进程或下一版加 `--port` flag

**为何不用 open / xdg-open**：单线程 CLI 需要等浏览器跳完 callback，必须自己 listen；浏览器打开方式见 §4.6。

### 3.3 Decimal helper 单一来源

把 `apps/elec/models.rs:109-144` 的 `decimal_str_or_num` mod 提取到 `src/util/decimal.rs`，elec 改 `use crate::util::decimal::decimal_str_or_num;`。

**改动面**：elec/models.rs 改 6 处 `with = "..."` 路径 + 删除 109-144 行 mod；新 `src/util/decimal.rs` ~50 行；`src/util/mod.rs` +1 pub mod 行。

**Trade-off**：
- 不改 elec：card/models.rs 复制一份 mod，~40 行重复；
- 改 elec：单一来源，但 elec 71 unit tests 全部 re-run 验证 import 无误。

选**改 elec**：元数据成本极低（plan 第一个 task，30 行修改 + 跑 test），换长期单一来源。

### 3.4 Token 落盘文件 schema

`~/.sjtu-cli/sub_sessions/card_oauth.json`（chmod 600 Unix；Windows ACL 仍 TODO 继承 S0 留白）：

```json
{
  "client_id": "...",
  "access_token": "...",
  "refresh_token": "...",
  "expires_at": "2026-05-17T15:00:00+08:00",
  "scope": "card_info card_transactions",
  "main_card_no": "12345",
  "captured_at": "2026-05-17T14:30:00+08:00"
}
```

**为什么不复用 `cookies::Session`**：Session struct 是 `name/value/domain/path` 形态的 cookie 列表，token 不是 cookie 形态。新 struct `CardOAuthSession` 显式表达 OAuth2 语义。

**client_id 存哪里**：与 token 同文件（公开信息，方便 refresh 用）。**client_secret 独立** `~/.sjtu-cli/card_oauth_secret.txt` chmod 600（CLI 启动时读，绝不落入 sub_session JSON，防止误传到外部）。

### 3.5 命令层 schema 演进策略

`Envelope<T>` 的 `schema_version="1"` 全项目共享。card 子系统 `BalanceData` / `HistoryData` 用 additive evolution：

```rust
pub struct BalanceData {
    pub card_no_redacted: String,    // "1234***"
    pub balance: Decimal,            // "284.25"
    pub trans_balance: Decimal,      // "0.00"
    pub expire_date: String,         // "20600101"（yyyyMMdd，按官方文档透传）
    pub lost: bool,
    pub frozen: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<UserIdentity>,  // --with-identity 才填，默认 None 整字段 skip
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bank_no_redacted: Option<String>, // 同上，--with-identity 才出（默认隐藏，不留 *** 占位）
    pub from_cache: bool,            // OAuth2 token 是否命中（与 cas/oauth2 envelope 对齐）
    pub elapsed_ms: u128,
}
```

---

## 4. 文件布局

### 4.1 新文件清单（17 文件 + 4 修改 + 5 测试 fixture）

```
src/
├── error.rs                                # +3 行：CardOAuth(String) variant + code() arm
├── lib.rs                                  # +0：apps/auth 已通过 mod re-export
├── util/
│   ├── mod.rs                              # +1：pub mod decimal
│   └── decimal.rs (NEW)                    # ~50：decimal_str_or_num 从 elec/models 迁来
├── auth/oauth2_dev/ (NEW dir)
│   ├── mod.rs                              # ~50：CardOAuthSession struct + load/save/exchange/refresh 顶层 API
│   ├── authorize.rs                        # ~70：authorize URL 构造 + state 生成 + 浏览器打开
│   ├── callback.rs                         # ~80：本地 server 接 callback + code 解析
│   ├── token.rs                            # ~70：POST /oauth2/token (code + refresh_token 两种 grant) + 响应解析
│   ├── refresh.rs                          # ~50：with_token_refresh<F,Fut,T> 同构 canvas_video::with_token_refresh
│   ├── secret.rs                           # ~30：读 ~/.sjtu-cli/card_oauth_secret.txt
│   ├── tests_token.rs                      # ~80：mockito 模拟 /oauth2/token 200/400/refresh round-trip
│   └── tests_callback.rs                   # ~60：本地 server 端到端 round-trip（hyper test client）
├── apps/card/ (NEW dir)
│   ├── mod.rs                              # ~25：pub use api::Client + LoginMeta + models
│   ├── api.rs                              # ~90：Client + connect + get_balance + get_transactions
│   ├── http.rs                             # ~110：build_http_client + fetch_json (Bearer 头 / 401 detect)
│   ├── models.rs                           # ~110：CardInfo / Transaction + serde rename
│   ├── throttle.rs                         # ~40：复制 elec/throttle 改 MIN_INTERVAL_MS=400（card_transactions 文档无明示，保守 400ms）
│   └── tests_parse.rs                      # ~100：CardInfo / Transactions fixture round-trip + dateTimAccount 拼写陷阱测试
└── commands/card/ (NEW dir)
    ├── mod.rs                              # ~10：pub use
    ├── data.rs                             # ~75：BalanceData + HistoryData + UserIdentity
    └── handlers.rs                         # ~110：cmd_balance + cmd_history + redact_identity
└── cli/card.rs (NEW)                       # ~50：CardSub enum (Balance/History) + dispatch
└── apps/elec/models.rs                     # -36 行：删 decimal_str_or_num mod，import 切 util
└── cli/mod.rs                              # +2 行：Card variant + arm
└── lib.rs                                  # +0
```

### 4.2 行数预算

| 区段 | 文件数 | 总行数 | 200 行硬限符合性 |
|---|---|---|---|
| 新 `util/decimal.rs` | 1 | ~50 | ✓ |
| 新 `auth/oauth2_dev/` | 8 (含 2 tests) | ~490 | 每文件 < 90，远在限内 |
| 新 `apps/card/` | 6 | ~475 | 最长 `http.rs:110` 与 `models.rs:110` 均 < 200 |
| 新 `commands/card/` | 3 | ~195 | `handlers.rs:110` < 200 |
| 新 `cli/card.rs` | 1 | ~50 | ✓ |
| 修改 elec/models.rs | -36 | (净) | 144 → ~108，更瘦 |
| **合计** | **+17 new / 4 mod** | **~1260 行新增** | 全 < 200 |

### 4.3 模块依赖箭头

```
cli/card.rs → commands/card/handlers.rs
                ↓
              apps/card/api.rs ← auth/oauth2_dev/{mod,refresh}.rs
                ↓                     ↓
              apps/card/http.rs   auth/oauth2_dev/{authorize,callback,token,secret}.rs
                ↓                     ↓
              apps/card/models.rs    cookies::{load_sub_session, save_sub_session}
                ↓                     ↓
              util/decimal.rs        error::SjtuCliError
```

无循环依赖；`apps/elec/models.rs` 改后 `→ util/decimal.rs`，与 card 并列消费者。

---

## 5. 核心数据契约

### 5.1 端点 1：`GET /v1/me/card`

- URL: `https://api.sjtu.edu.cn/v1/me/card`
- Auth: query `?access_token=<TOKEN>` **或** header `Authorization: Bearer <TOKEN>` —— spec 选 **header**（更标准、URL 不带敏感数据）
- scope: `card_info`
- 参数: 无
- 错误形态: `HTTP 200 + errno=10002 / error="Authentication Failed"`（不是 401）→ 触发 token refresh

### 5.2 端点 1 响应字段表

| 字段 | 类型 | Rust 字段名 | 默认输出？ |
|---|---|---|---|
| `user.code` | string | `user.code` | 仅 `--with-identity` |
| `user.name` | string | `user.name` | 仅 `--with-identity` |
| `user.organize.name` | string | `user.organize.name` | 仅 `--with-identity` |
| `cardNo` | string | `card_no` | 脱敏首 4 位 + `***` |
| `cardId` | string | `card_id_redacted` | 永久不出（物理卡号 PII） |
| `bankNo` | string | `bank_no` | 仅 `--with-identity` |
| `expireDate` | string (yyyyMMdd) | `expire_date` | 透传字符串 |
| `cardBalance` | double → Decimal | `balance` | 默认出 |
| `transBalance` | double → Decimal | `trans_balance` | 默认出 |
| `lost` | bool | `lost` | 默认出 |
| `frozen` | bool | `frozen` | 默认出 |
| `faceType` | string | `face_type` | 默认出（类型代码无 PII） |
| `faceSubType` | string | `face_sub_type` | 仅 `--with-identity`（含"硕士研究生"等身份描述） |

### 5.3 端点 2：`GET /v1/me/card/transactions`

- scope: `card_transactions`
- 参数:
  - `cardNo` = main_card_no（从 `card_oauth.json` 读，首跑由 `/v1/me/card` 提取）
  - `beginDate` = `(today - N days).timestamp_millis()`（CLI `--days N`，默认 30）
  - `endDate` = `today.timestamp_millis()`
  - `orderBy` = `dateTime`（默认按消费时间）
  - `start` = 0
  - `limit` = `min(--limit, 100)`（默认 50，硬限 100）

### 5.4 端点 2 响应字段表

| 字段 | 类型 | Rust 字段名 | 备注 |
|---|---|---|---|
| `dateTime` | long (ms_ts) | `consumed_at` (DateTime<FixedOffset>) | 转 `+08:00` |
| `dateTimAccount` | long (ms_ts) | `accounted_at: Option<...>` | ⚠️ `#[serde(rename = "dateTimAccount")]`（少 e），仅 orderBy=dateTimeAccount 返 |
| `system` | string | `system` | 透传 |
| `merchantNo` | string | `merchant_no` | 透传 |
| `merchant` | string | `merchant` | 透传 |
| `description` | string | `description` | 透传 |
| `amount` | double → Decimal | `amount` | 消费为负 |
| `cardBalance` | double → Decimal | `balance_after` | 交易后余额 |

### 5.5 历史输出 envelope

`HistoryData` 包含：
- `card_no_redacted`（脱敏）
- `begin_date_local: NaiveDate` / `end_date_local: NaiveDate`
- `returned: usize` / `total: u64`（服务端报）
- `transactions: Vec<TransactionItem>`
- `total_amount: Decimal`（消费汇总，负数；用 Decimal 链式累加，参 elec history `total_kwh`）
- `from_cache: bool` / `elapsed_ms: u128`

---

## 6. OAuth2 流程时序

### 6.1 首次授权（无 access_token）

```text
1. CLI: 读 ~/.sjtu-cli/card_oauth_config.toml 拿 client_id
2. CLI: 读 ~/.sjtu-cli/card_oauth_secret.txt 拿 client_secret（chmod 600 检查）
3. CLI: 生成 state = base64(rand 32 bytes)
4. CLI: bind 127.0.0.1:45123 启 callback listener
5. CLI: 构造 authorize URL =
   https://jaccount.sjtu.edu.cn/oauth2/authorize?
     response_type=code&
     client_id=<ID>&
     redirect_uri=http://127.0.0.1:45123/callback&
     scope=card_info+card_transactions&
     state=<STATE>
6. CLI: 用 headless_chrome (复用 S1 依赖) 打开 authorize URL；用户已 jAccount 登录 → 直接进入"授权 sjtu-cli 访问一卡通信息"确认页 → 点同意
   备选: 用 std::process::Command 调系统默认浏览器 (windows: cmd /c start <URL>) — 比 headless_chrome 轻，但用户必须自己在浏览器里 jAccount 登录 — spec 首版选 headless_chrome 复用主 session
7. 浏览器 302 → http://127.0.0.1:45123/callback?code=<CODE>&state=<STATE>
8. CLI listener: 解析 HTTP/1.1 GET 第一行的 query，验证 state 匹配
9. CLI: POST jaccount.sjtu.edu.cn/oauth2/token，body =
   grant_type=authorization_code&
   code=<CODE>&
   redirect_uri=http://127.0.0.1:45123/callback&
   client_id=<ID>&
   client_secret=<SECRET>
10. 服务端 200: { expires_in:1800, token_type:Bearer, refresh_token, access_token }
11. CLI: 算 expires_at = now + 1800s - 60s safety margin
12. CLI: 首跑 GET /v1/me/card 拿主卡 cardNo (entities[0].cardNo)，写入 main_card_no
13. CLI: 落盘 ~/.sjtu-cli/sub_sessions/card_oauth.json (chmod 600)
14. CLI: listener 返 200 OK + HTML "授权成功，可关闭窗口"；CLI 主流程继续
```

### 6.2 后续命令（access_token 仍 fresh）

```text
1. CLI: load_sub_session("card_oauth") → CardOAuthSession
2. CLI: now < expires_at → fresh，直接走 §6.4 API 调用
```

### 6.3 access_token 过期（refresh）

```text
1. CLI: now >= expires_at OR 上一次 API 返 errno=10002
2. CLI: POST /oauth2/token, body =
   grant_type=refresh_token&
   refresh_token=<OLD>&
   client_id=<ID>&
   client_secret=<SECRET>
3. 服务端: { expires_in:1800, access_token, refresh_token (可能换可能不换) }
4. CLI: 更新 card_oauth.json，重试一次原 API 调用
```

### 6.4 API 调用（with_token_refresh 包裹）

```rust
// commands/card/handlers.rs
let envelope = with_token_refresh(|| async {
    let client = Client::connect().await?;
    client.get_balance().await
}).await?;
```

`with_token_refresh<F,Fut,T>` 内部：

```rust
match op().await {
    Ok(v) => Ok(v),
    Err(e) if is_token_expired(&e) => {
        oauth2_dev::refresh().await?;
        op().await  // 重试一次
    }
    Err(e) => Err(e),
}
```

`is_token_expired` 判定：anyhow downcast `SjtuCliError::CardOAuth("token_expired" | "errno_10002")`。

---

## 7. 错误处理

### 7.1 `SjtuCliError` 新 variants

```rust
#[error("一卡通 OAuth2: {0}")]
CardOAuth(String),                       // 通用，含 "token_expired" / "secret_missing" 等

#[error("一卡通 client_secret 未配置，请把 client_secret 写入 ~/.sjtu-cli/card_oauth_secret.txt 后重试")]
CardOAuthSecretMissing,

#[error("一卡通授权流程超时，请重试 `sjtu card auth`")]
CardOAuthTimeout,
```

### 7.2 错误码映射（output.rs `code()`）

| Variant | code | exit code |
|---|---|---|
| `CardOAuth("token_expired")` | `session_expired` | 0（命令层吃 retry 自动续，不会暴露给用户） |
| `CardOAuth("port_in_use")` | `port_in_use` | 1 |
| `CardOAuthSecretMissing` | `config_missing` | 1 |
| `CardOAuthTimeout` | `auth_timeout` | 1 |

---

## 8. PII 默认脱敏 + `--with-identity`

| 字段 | 默认 | `--with-identity` |
|---|---|---|
| `cardNo` | `1234***` 前 4 位 | 全 |
| `cardId` (物理卡号) | **永久 None**（写不在 JSON 里） | **永久 None** |
| `bankNo` (银行卡号) | None | 前 4 + `****` + 后 4 |
| `user.code` (学号) | None | 全 |
| `user.name` (姓名) | None | 全 |
| `user.organize.name` | None | 全 |
| `faceSubType`（如"硕士研究生"） | None | 全 |
| `expireDate` | 全（公开有效期信息） | 全 |
| `cardBalance` / `transBalance` | 全 | 全 |
| `lost` / `frozen` | 全 | 全 |
| `merchant` / `description`（消费记录） | 全（商户名属公开消费信息） | 全 |

**红线**：物理卡号 `cardId` 即便 `--with-identity` 也不出（防卡号克隆攻击面）。

---

## 9. 测试策略

### 9.1 单元测试（mockito，不打真服务器）

| 测试 | 文件 | 用 mockito 模拟什么 |
|---|---|---|
| `oauth2_dev::token::exchange_code_for_token` | tests_token.rs | POST `/oauth2/token` 返 200 expected JSON |
| `oauth2_dev::token::refresh` | tests_token.rs | POST `/oauth2/token` `grant_type=refresh_token` round-trip |
| `oauth2_dev::token::refresh_invalid` | tests_token.rs | 服务端 400 `invalid_grant` → 错误向上 |
| `oauth2_dev::callback::extract_code` | tests_callback.rs | 本地 server start → curl localhost:45123/callback?code=X&state=Y → server 解析 |
| `oauth2_dev::callback::state_mismatch` | tests_callback.rs | state 不匹配 → 报错 + 拒绝 |
| `apps::card::parse_card_info` | tests_parse.rs | `entities[0]` 反序列化字段全过 |
| `apps::card::parse_transactions` | tests_parse.rs | 多条 entities + `dateTimAccount` 拼写测试 |
| `apps::card::parse_transactions_empty` | tests_parse.rs | `total=0 entities=[]` 路径 |
| `apps::card::parse_balance_neg_amount` | tests_parse.rs | `amount=-10.66` 反序列化为 `Decimal("-10.66")` |
| `with_token_refresh::happy` | tests_token.rs | op 第一次 OK 不触发 refresh |
| `with_token_refresh::refresh_on_expired` | tests_token.rs | op 第一次 errno=10002 → refresh → 第二次 OK |
| `with_token_refresh::refresh_fails` | tests_token.rs | refresh 也 fail → 不再重试，向上抛 |

预计 12 个新测试 + elec/models 现有 71 个测试 re-run 验证 import 切换无误。

### 9.2 真机 checkpoint（clientId 拿到后）

| ID | 命令 | 验证点 |
|---|---|---|
| **CP-T4-AUTH** | `sjtu card auth`（隐藏命令，仅首次） | 浏览器弹 jAccount 授权 → 同意 → callback 拿 code → token 落 `card_oauth.json` |
| **CP-T4-BAL** | `sjtu card balance --json` | balance/trans_balance 是字符串形 Decimal；card_no 脱敏 `1234***`；user/bank_no 字段不出 |
| **CP-T4-BAL-ID** | `sjtu card balance --with-identity --json` | user.name + bank_no 出全；card_id 仍不出 |
| **CP-T4-HIST** | `sjtu card history --days 7 --json` | transactions 数组；金额负值；total_amount 累加精确（手算复核） |
| **CP-T4-HIST-EMPTY** | `sjtu card history --days 1 --json`（若今日无消费） | `total: 0`、`returned: 0`、不 panic |
| **CP-T4-REFRESH** | 等 31 分钟后跑 `sjtu card balance` | log 显示触发 refresh，命令仍成功 |
| **CP-T4-LIMIT** | `sjtu card history --days 365 --limit 200` | 服务端硬限 100，CLI clamp 应起作用，无服务端 4xx |

### 9.3 fmt / clippy / build 门禁

每个 commit 前必过：

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
cargo build --release
```

---

## 10. 联网验证综合（2026-05-17）

### 10.1 业界 idiomatic 路径对照

| 维度 | 业界 Rust 2026 标准 | 本 spec 选 |
|---|---|---|
| OAuth2 库 | `oauth2` crate (1.x) | **手卷** — 仅 2 个 endpoint，新增 ~120 行 vs +1 crate；遵守 CLAUDE.md "不引入新依赖" |
| Token storage | `keyring` crate (OS-native 凭据) | **JSON 文件 chmod 600** — 与 cookies::session.json 同制；keyring 跨平台行为不一致（Windows DPAPI vs Linux Secret Service） |
| 本地 callback server | `axum` / `warp` micro-framework | **手卷 `TcpListener` + 1 个 GET 解析** ~60 行；axum 仅为 1 个 endpoint 引入是大杀器 |
| Refresh 策略 | Eager (定时 refresh) | **Lazy (failure-driven)** — 与 canvas_video::with_token_refresh 同构；省状态机 |
| PKCE | RFC7636 推荐 | **首版不实装**（OQ-1 待确认） |

### 10.2 关键 trade-off doc-in-code

每个手卷选择必须在 `auth/oauth2_dev/mod.rs` 顶部 module doc 写出来：

```rust
//! T4 一卡通 OAuth2 Authorization Code 通道。
//!
//! 不用 `oauth2` crate（违 CLAUDE.md 不引入新依赖）。
//! 不用 `keyring`（跨平台行为不一致；JSON+chmod 600 与 cookies::session.json 同制，单一可审计点）。
//! 不用 `axum`（1 endpoint 不值得引入 micro-framework；手卷 60 行 listener 够用）。
//! Refresh 走 failure-driven 不走 timer（同 canvas_video::with_token_refresh 范式，省状态机）。
```

---

## 11. Risks / Open Questions

### Risks

| ID | 描述 | 缓解 |
|---|---|---|
| R1 | clientId 审批被拒 / 周期 > 3 工作日 | 不阻塞 spec/plan/TDD；mockito 测试可独立跑；若被拒走 phase-2 重申请 + 调整描述 |
| R2 | redirect_uri 服务端要求精确匹配且不允许 127.0.0.1 | 申请时填 `http://localhost:45123/callback` 备选；若都不允许走 phase-2 评估是否走 OOB（`urn:ietf:wg:oauth:2.0:oob`） |
| R3 | `card_transactions` scope 拒批（学校怀疑滥用） | 申请描述强调"个人只读 + 本机展示 + 无聚合上传"；若拒批仅做 balance MVP |
| R4 | `bankNo` 字段在用户已绑定银行卡时含真实卡号 → 即便 `--with-identity` 都需脱敏处理 | spec §8 已决定：`bank_no` 即便 `--with-identity` 也只出前 4 + `****` + 后 4 |
| R5 | 服务端 1800s 过期改成 60s 或类似激进 TTL | with_token_refresh 已经处理；R5 实际无影响 |
| R6 | headless_chrome 在 Linux 无桌面环境下不能弹浏览器 | spec 留 `--manual-auth` flag：打印 authorize URL + 提示用户在任意浏览器打开 + 输入 callback URL，由 CLI stdin 解析 code（phase-2） |

### Open Questions

- **OQ-1**: PKCE 服务端是否强制？申请审批后第一次 authorize 跑通即可确认；若强制则 plan 加 1 task 补 `code_challenge` + `code_verifier`
- **OQ-2**: redirect_uri 申请表单允许填几个？多个的话填两个备选 `127.0.0.1:45123/callback` + `localhost:45123/callback`，避开 host 解析差异
- **OQ-3**: refresh_token 是否一次性？若每次 refresh 返回新 refresh_token，spec 已支持；若是长期不变的，存储语义不变（覆写即可）
- **OQ-4**: `bus` scope 是否包含校车实时？与本 spec 无关，但若未来要加 `sjtu bus` 命令可复用 `oauth2_dev` 通道
- **OQ-5**: T8 CAS retry 层 follow-up（elec/services/jwbmessage 接入）与本 spec 完全解耦；不在本 spec 范围
- **OQ-6**: 是否需要 `sjtu card revoke` 子命令调 Logout URL? 不在 MVP；用户可手 `rm ~/.sjtu-cli/sub_sessions/card_oauth.json` 等价

---

## 12. clientId 申请用户引导（重提，简版）

详细见 `docs/superpowers/research/2026-05-17-t4-update.md §4`。要点：

1. 登录 `developer.sjtu.edu.cn` 用 jAccount
2. 新建应用：
   - 名称：`sjtu-cli 一卡通查询`
   - 类型：桌面 / CLI / 命令行（看下拉）
   - 描述：包含**"仅一卡通余额 + 消费记录只读展示"**
   - redirect_uri：`http://127.0.0.1:45123/callback`
   - scope：勾选 **`card_info` + `card_transactions`**
   - 授权模式：**Authorization Code**
3. 提交，等 3 工作日
4. 批准后把 `client_id` 告诉我（公开），自己把 `client_secret` 写入 `~/.sjtu-cli/card_oauth_secret.txt` chmod 600
5. **不要把 `client_secret` 告诉我或贴到对话里** —— 它落本机 600 文件即可，CLI 启动时自动读

---

## 13. 阶段化交付（高层；详细 task 留 plan）

```
Plan task 序列预估 (TDD + commit gates):

T1  util/decimal.rs 提取 + elec import 切换 (~30 行) → cargo test 71 全绿
T2  error.rs +3 variants → cargo build
T3  auth/oauth2_dev/secret.rs + tests → 单测
T4  auth/oauth2_dev/token.rs (exchange_code + refresh) + tests_token.rs → mockito
T5  auth/oauth2_dev/callback.rs + tests_callback.rs → 本地 server e2e
T6  auth/oauth2_dev/authorize.rs (无 tests，只是 URL build) + headless_chrome glue
T7  auth/oauth2_dev/refresh.rs::with_token_refresh<F,Fut,T> + 3 tests
T8  auth/oauth2_dev/mod.rs 顶层 API (CardOAuthSession + load/save/get_or_refresh)
T9  apps/card/{mod,api,http,models,throttle,tests_parse}.rs (mockito + fixture)
T10 commands/card/{mod,data,handlers}.rs (cmd_balance + cmd_history + redact)
T11 cli/card.rs + cli/mod.rs dispatch
T12 真机 CP-T4-AUTH (等 clientId)
T13 真机 CP-T4-BAL / BAL-ID / HIST / HIST-EMPTY
T14 真机 CP-T4-REFRESH (等 30+min 验证 lazy refresh)
T15 文档收尾: tasks/todo.md + lessons.md + README.md + SKILL.md + CLAUDE.md
```

T1-T11 可在 clientId 审批期间并行完成（mockito 不打真服务器）；T12-T14 阻塞 clientId。

---

## 14. 验收

- Spec 过审 by 用户 → 转 writing-plans（明日或申请提交后）
- TDD 紧跟 plan，commit-by-commit gate
- 4 关真机 checkpoint 全过 + lessons.md 记录 1-3 条 Rust/OAuth2 教训 → T4 收尾

---

> 本 spec 不含任何真实 cookie / 学号 / 余额 / banking number / client_secret。
