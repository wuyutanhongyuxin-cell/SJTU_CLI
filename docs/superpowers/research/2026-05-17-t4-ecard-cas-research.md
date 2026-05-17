# T4 一卡通 — ecard CAS Fallback 路径预研补丁

> 2026-05-17。基线：
> - `2026-05-15-t4-ecard-prerequisites.md`（原始预研）
> - `2026-05-17-t4-update.md`（OAuth2 路径锁定）
>
> 本补丁：在 OAuth2 `client_id` 审批阻塞背景下，加 `ecard.sjtu.edu.cn` CAS 路径作 fallback，
> 形成 SJTU-CLI 一卡通**双轨**鉴权架构。

---

## 0. 决策回顾（用户已确认）

| 项 | 锁定 |
|---|---|
| **架构** | 双轨：OAuth2 (`api.sjtu.edu.cn`) 为主 + ecard CAS 为兜底 |
| **Fallback 触发** | **仅** `CardOAuthSecretMissing` / 未配置 `client_id` 时**透明** fallback；其它 OAuth2 运行时失败（`token_expired` + refresh fail / 5xx / scope deny）**不**自动 fallback，正常抛错让用户感知 |
| **CLI flag** | 新增 `--via {oauth2, cas, auto}`，`auto`（默认）= 上述自动 fallback 逻辑；`oauth2` / `cas` 强制单轨 |
| **scope** | ecard CAS 路径不涉及 OAuth2 scope；OAuth2 path 仍 `card_info + card_transactions` 不变 |
| **金额** | 仍 `rust_decimal::Decimal`；CAS path 哪怕返回字符串/HTML 也强转 Decimal，绝不 f32/f64 |
| **写端点** | 永久红线，CAS path 也不实装任何写端点 |

---

## 1. 网络环境前提（双轨可用域）

| 域 | 公网状态（4 月观察 + 2026-05-17 复测） | 适用 path |
|---|---|---|
| `ecard.sjtu.edu.cn` | `302 → restrict.sjtu.edu.cn` | **仅校园网** |
| `api.sjtu.edu.cn` | 公网可达，需 OAuth2 Bearer | 全网（OAuth2 path） |

**CAS path 固有约束**：用户在校外/出差时 `--via cas` **直接超时/失败**（`restrict.sjtu.edu.cn` 不可达）。
这是 fallback 不是 primary 的**核心原因** —— 若用户在校外**且**没有 `client_id`，本工具瘫痪；必须透明告知（错误消息建议措辞：`未配置 OAuth2 client_id 且当前不在校园网内 — 请走 OAuth2 申请流程，或接入校园网后用 --via cas`）。

---

## 2. Open Questions（必须校园网内调研回填）

### 红线复诵（调研期）
按 `i.sjtu / 交我办` 硬红线：**只读**调研，仅允许 `take_snapshot` / `take_screenshot` / `list_network_requests` / `get_network_request` / `evaluate_script`（read-only JS，**禁** `form.submit()` / POST / PUT / DELETE）。
**严禁**点击任何"提交 / 修改 / 退订 / 充值 / 挂失 / 改密码 / 拾卡 / 改照片" 按钮。
即便已 CAS 登入，按"只读访客"操作。

### OQ-CAS-1 — ecard 入口 + CAS 跳转链

**调研步骤**：
1. 校园网内打开浏览器 → `mcp__chrome-devtools__new_page` URL `https://ecard.sjtu.edu.cn/`
2. `mcp__chrome-devtools__list_network_requests` 记录完整 redirect 链
3. 期望形态：`ecard.sjtu.edu.cn` → `jaccount.sjtu.edu.cn/jaccount/jalogin?...` → 回 `ecard.sjtu.edu.cn/.../?ticket=ST-...`
4. 落地后 `take_snapshot` 看主页结构

**回填**：完整 redirect URL 链 + cookie 列表（仅 `name + domain + path + expiry`，**不贴 value**）

### OQ-CAS-2 — 余额查询端点

**调研步骤**（CAS 已通后）：
1. ecard 主页找"余额查询" / "卡余额" / "账户信息"入口（**不要**点充值/挂失任何按钮）
2. 点入查询页（这是查询，不是提交，符合红线）
3. `list_network_requests` 抓所有 XHR/fetch
4. `get_network_request` 取关键请求的 body 看 response Content-Type + 字段名

**回填**：endpoint URL、HTTP method、form/query param、response 类型（JSON / HTML / XML）、关键字段名（`cardBalance` / `余额` / `账户余额` 等）

### OQ-CAS-3 — 消费记录端点

**调研步骤**：
1. 找"消费记录" / "交易明细" / "流水查询"
2. `list_network_requests` 抓 XHR
3. 注意分页（offset/limit or page/size）+ 时间过滤参数（beginDate/endDate）
4. 注意单页条数上限观察

**回填**：endpoint URL、分页参数、time range 参数、单条记录字段（`amount` / `merchant` / `dateTime` 同 OAuth2？）

### OQ-CAS-4 — Session stale 形态

**调研步骤**：
1. CAS 链通后**主动等 30 分钟**，或开新 Incognito 不带 cookie 直接打余额 endpoint
2. 重新 GET 余额端点
3. 观察 stale 响应

**回填**：stale 时 HTTP status、是否 302 → login、body snippet（无 PII，前 200 字符即可）

### OQ-CAS-5 — 多卡场景

OAuth2 path 已锁主卡（`GET /v1/me/card` 拿 cardNo）。CAS path 是否同样需要 cardNo 参数？默认主卡如何确定？

**回填**：余额 endpoint 是否要 cardNo 入参；如要，默认值规则。

### OQ-CAS-6 — Cookie 落地形态

**回填**：ecard CAS 后 cookie 落 `~/.sjtu-cli/sub_sessions/ecard.json`（仿 `elec` / `jwc` 同款）即可，还是有特殊 cookie 域跨域问题？

---

## 3. 架构草案（spec 前预演，spec 阶段精化）

### 3.1 文件骨架

```text
src/apps/card/
├── mod.rs                # 路由：根据 --via + config 选 path
├── api.rs                # OAuth2 path（已有，不动）
├── http.rs               # OAuth2 HTTP（已有，不动）
├── oauth_dev/            # OAuth2 path（已有，不动）
├── cas/                  # NEW — ecard CAS 路径
│   ├── mod.rs            # cookie+CAS 入口
│   ├── client.rs         # reqwest Client w/ cookie jar
│   ├── balance.rs        # 余额查询
│   ├── transactions.rs   # 消费记录
│   └── tests.rs          # mockito 单测
├── via.rs                # NEW — VIA enum + 路径选择器
├── models/               # 复用（OAuth2 / CAS 同 struct，差异字段 #[serde(default)]）
└── ...
```

### 3.2 鉴权 fallback 流（D 模式）

```text
sjtu card balance [--via auto|oauth2|cas]
    └─ --via auto (default):
        ├─ try OAuth2 path
        │   ├─ ok → 走 OAuth2（envelope.meta.via="oauth2"）
        │   ├─ CardOAuthSecretMissing → fallback CAS（meta.via="cas", meta.fallback_reason="oauth2_not_configured"）
        │   └─ 其它错误（token_expired refresh fail / 5xx / scope deny）→ 抛错（不 fallback）
        └─ --via oauth2: 强制 OAuth2，任何错误透传
        └─ --via cas: 强制 CAS（用户主动选）
```

### 3.3 Envelope `meta.via` 字段

新增 `meta.via: "oauth2" | "cas"` —— **不放 data 内层**，避免污染数据 schema。
fallback 触发时同时填 `meta.fallback_reason: "oauth2_not_configured"`。
所有 card 子命令都填 `meta.via` —— 让 Agent / 用户透明感知本次走的哪条（debug 友好）。

### 3.4 复用 vs 新建

| 复用 | 新建 |
|---|---|
| `models/card_record.rs`（字段同形 `#[serde(default)]` 容差） | `cas/` 子目录全部 |
| `Envelope<T>` + serde bound 防 `T: Default` 传染 | `via.rs` + `--via` clap derive |
| CLAUDE.md 红线（金额 Decimal / 写端点禁） | `meta.via` Envelope 字段 |
| `commands/card/` handlers 顶层路由 | `commands/card/dispatcher.rs`（路径选择） |

### 3.5 与 CAS retry 层（T8）的耦合

CAS path **复用** `src/auth/cas/retry::with_cas_refresh<F>` —— 同 `apps/elec`：
- ecard cookie session stale 时，由 `with_cas_refresh` 捕获 `SubSessionStale("card_cas")` 重 CAS。
- 注意：与 OAuth2 path 的 `with_token_refresh`（refresh_token 续期）是**两套独立机制**，不混用。

---

## 4. 风险 + 不阻塞项

### 阻塞（必须 OQ 回填）
- OQ-CAS-1 / 2 / 3 / 4：spec 端点签名 + 错误形态写不了
- OQ-CAS-5 / 6：可临时假设（多卡 = OAuth2 同款；cookie 单独域），spec 草案标 TBC 不阻塞

### 不阻塞（可并行做）
- CLI flag 设计 / `--via` clap derive 写法
- Envelope `meta.via` / `meta.fallback_reason` 字段命名 + 序列化 fixture
- mockito 单测骨架（按 OQ 回填后的契约填实）
- 文档（CLAUDE.md 加 `--via` 约束 / SKILL.md 加用法 / README card 表格补一栏）

---

## 5. 红线复诵（一卡通永久不实装）

| 端点 | CAS path 立场 |
|---|---|
| 充值 / 挂失 / 解挂 / 改密码 / 改照片 / 拾卡 | ✗ 不实装 |
| 个人信息维护（手机 / 地址 / 银行卡） | ✗ 不实装 |
| 圈存 / 转账 / 退款 | ✗ 不实装 |

CAS path 仅读：**余额 + 消费记录**。即便 ecard 页面有按钮，也绝不调任何 POST / PUT / DELETE。

---

## 6. 下一步

1. ⏳ **用户校园网内跑 OQ-CAS-1～4 调研脚本**（当前阻塞 — 见 §2）
2. ⏳ 我据此写 spec `docs/superpowers/specs/2026-05-17-t4-ecard-cas-fallback-design.md`
3. ⏳ spec 过审 → 写 plan `docs/superpowers/plans/2026-05-17-t4-ecard-cas-fallback.md`
4. ⏳ subagent-driven execute（fresh subagent per task + two-stage review）

---

> 本预研不含真实 cookie / 学号 / 余额 / banking 数据。CAS path 永久绑校园网是已知约束，文档已固化在 spec 之前。
