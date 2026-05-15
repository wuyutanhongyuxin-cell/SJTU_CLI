# T6 图书馆借阅列表预研报告

> 日期：2026-05-15 | 阶段：纯预研，不含实装代码


## §1 现有 CAS 子系统先例分析

### 1.1 新 CAS 子系统标准骨架

基于 `apps/elec/` 先例，新 `apps/library/` 目录结构如下：

```
src/apps/library/
├── mod.rs          # pub use api::{Client, LoginMeta}; pub use models::{...}
├── api/
│   ├── mod.rs      # Client struct + connect() + 各端点 impl
│   └── loans.rs    # 当前借阅列表 + 历史借阅（各 < 80 行）
├── http.rs         # build_http_client(&session) + fetch_json/fetch_html
├── models/
│   ├── mod.rs      # pub use loan::LoanRecord; pub use item::BookItem
│   ├── loan.rs     # LoanRecord { isbn, title, author, due_date, ... }
│   └── item.rs     # BookItem（书目详情，T7 接口留空字段）
└── tests_parse.rs  # mockito 单测（html / json fixture）
```

### 1.2 CAS connect() 样板（基于 elec 精简）

```rust
pub async fn connect() -> Result<Self> {
    let r = cas_login("library", LOGIN_URL).await?;
    let http = build_http_client(&r.session)?;
    Ok(Self { http, throttle: Arc::new(Throttle::new()),
              login: LoginMeta { from_cache: r.from_cache,
                                 elapsed_ms: r.elapsed_ms,
                                 final_url: r.final_url } })
}
```

`cas_login("library", LOGIN_URL)` 落盘到 `~/.sjtu-cli/sub_sessions/library.json`，
命中缓存检查 `captured_at >= main.captured_at`（T12 staleness fix 已内置）。

### 1.3 Envelope 跨子系统统一约束

- 所有命令层输出走 `output::Envelope<T>` (`ok / schema_version / data / error`)
- 失败时附 `error.raw_snippet`（前 300 字节），不 panic
- 借阅数量字段用 `u32`；罚款/押金字段若存在用 `rust_decimal::Decimal` + 字符串序列化

---

## §2 图书馆后端 Unknowns

### 2.1 入口域名候选（未确认，需真机）

| 候选 | 依据 | 置信度 |
|------|------|--------|
| `ilink.lib.sjtu.edu.cn` | 常见 SJTU 子域命名规律 | 低 |
| `opac.lib.sjtu.edu.cn` | 通用 OPAC 前缀惯例 | 低 |
| `www.lib.sjtu.edu.cn/patron/...` | 主域下个人服务路径 | 低 |
| 交我办/i.sjtu 内嵌 iframe | 公告页有"交我办"一卡通借阅跳转 | 中 |

WebSearch 确认 `www.lib.sjtu.edu.cn` 是主域；是否有独立 API 子域**未知，需真机**。

### 2.2 进入方式 Unknowns

- **CAS 跳转**（与 elec/jwc 同路）：最可能，jaccount SSO 集成最广
- **OAuth2 授权**：SJTU 开发者平台 `developer.sjtu.edu.cn` 有 jAccount OAuth2 标准流程，但图书馆是否已接入**未知**
- **直接 cookie 跨域**：my.sjtu 的 JAAuthCookie 或 UUKey 是否被 lib 域接受**未知**
- **完全独立登录**：不走 jaccount，用图书馆自己账号体系（低概率但要排除）

### 2.3 端点 Unknowns（需真机抓包确认）

| 端点用途 | 候选路径形态 | 未知点 |
|----------|-------------|--------|
| 当前借阅列表 | `GET /patron/loans` 或 `/api/v1/patron/loans` | 完整路径、分页参数 |
| 借阅历史 | `GET /patron/loanHistory` | 是否支持、时间范围参数 |
| 应还日期 | 同上 `due_date` 字段 | 字段名、时区、格式（ISO/中文） |
| 馆藏详情 | `/items/<id>` 或 `/bibs/<bib_id>` | 是否需要二次请求 join |
| 预约记录 | `GET /patron/requests` | 本轮不实装，仅了解路径 |

### 2.4 只读 Scope 划线（本轮 NOT IN SCOPE）

以下端点涉及写操作，**本轮严格禁止**，调研时也不点击对应按钮：
- 续借：`POST /patron/loans/<id>/renew`
- 预约：`POST /patron/requests`
- 取消预约：`DELETE /patron/requests/<id>`

### 2.5 数据模型 Unknowns

```rust
// 暂定草稿，真机确认后调整
pub struct LoanRecord {
    pub bib_id: String,          // 书目 ID（类型未知：数字/hash）
    pub title: String,
    pub author: Option<String>,
    pub call_number: Option<String>,  // 索书号
    pub location: Option<String>,     // 馆藏地点（"李文正馆 3F"）
    pub loan_date: Option<String>,    // 借出日期，格式未知
    pub due_date: Option<String>,     // 应还日期（T7 输入源）
    pub renewals: Option<u8>,         // 已续借次数
    pub barcode: Option<String>,      // 图书条码
}
```

字段名大小写（camelCase/snake_case/中文键）、日期格式（`YYYY-MM-DD` / Unix timestamp / 中文）**全部未知**。

---

## §3 WebSearch 公开资料

**未找到**任何反推 SJTU 图书馆私有 API 的公开博客或 GitHub 项目。

搜索结论：
- `www.lib.sjtu.edu.cn` 确认是图书馆主域（3 次搜索均命中）
- SJTU jAccount OAuth2 开发者平台存在（`developer.sjtu.edu.cn`），但图书馆服务是否注册其中**未知**
- 全球 OPAC 系统（Ex Libris Primo / Koha）有 REST API，但 SJTU 是否使用这些商业系统**未知**
- 无公开第三方反向工程项目（GitHub topics/opac 无 SJTU 相关仓库）

---

## §4 真机调研步骤（给主对话用户）

以下步骤使用 chrome-devtools，**只读操作**，不点续借/预约/取消任何按钮：

1. **打开 DevTools Network 面板**，勾选「Preserve log」，Filter 设为「XHR/Fetch」
2. **访问 `https://www.lib.sjtu.edu.cn`**，点个人账户入口（通常右上角图标），观察是否触发 302 跳转到 jaccount 或其他域
3. **登录后进入「我的图书馆」→「借阅列表」页**，截图 Network tab 里所有 XHR/Fetch 请求（特别关注 URL、method、status code）
4. **抓 Response body**：对借阅列表的请求调用 `get_network_request` 查看完整 JSON/HTML 结构；记录字段名和数据类型
5. **点击「借阅历史」（若存在）**，同样抓取 network request，确认是否独立端点还是同一端点加参数
6. **检查 Cookie**：用 `evaluate_script` 执行 `document.cookie` 查看 lib 域下的 session cookie 名称（只读，仅查看，不修改）
7. **记录最终 URL**：CAS 跳转链的落点域名（lib 域还是 my.sjtu 域），作为 `cas_login("library", "<落点URL>")` 的 `target_url` 参数
8. **截图备存**：借阅列表页面完整截图，确认字段（书名/作者/应还日期/索书号/馆藏地点）实际显示内容

---

## §5 实装前血路图

### 5.1 阻塞项（必须先解决）

| # | Unknown | 解决方式 |
|---|---------|---------|
| B1 | 入口域名 + 进入方式（CAS/OAuth/独立） | 真机步骤 §4 第 2、7 步 |
| B2 | 借阅列表端点路径 + 分页参数 | 真机步骤 §4 第 3、4 步 |
| B3 | 数据字段名 + 日期格式 | 真机步骤 §4 第 4 步，抓 JSON |
| B4 | Session cookie 名称（JSESSIONID / SESSION / 其他） | 真机步骤 §4 第 6 步 |

### 5.2 可延后项（不阻塞 MVP）

| # | Unknown | 说明 |
|---|---------|------|
| D1 | 借阅历史（翻页/时间范围） | MVP 先只做当前借阅 |
| D2 | 馆藏详情二次请求（索书号/馆位） | 若借阅列表已含则省略 |
| D3 | 罚款/押金字段（Decimal 处理） | 有则加，无则跳过 |

### 5.3 与 T8（CAS retry layer）的耦合

T8 计划引入 `cas_retry_client`（staleness-detect + auto re-login wrapper）。
`library::Client::connect()` **应等 T8 合并后**再实装，改为：

```rust
// T8 后的形态
let r = cas_login_with_retry("library", LOGIN_URL).await?;
```

若先于 T8 实装，用现有 `cas_login` 占位，留 `// TODO(T8): replace with retry helper` 注释。

### 5.4 与 T7（通知聚合）的耦合

T7「即将到期」通知需要 `LoanRecord.due_date` 作为输入源。
模型设计要求：
- `due_date` 暴露为 `Option<chrono::NaiveDate>`（非裸字符串），方便 T7 做日期比较
- `LoanRecord` derive `Serialize + Clone`，T7 可直接消费
- 建议在 `models/loan.rs` 加 `pub fn days_until_due(&self) -> Option<i64>` helper

---

## §6 200 行硬限预估

| 文件 | 预估行数 | 说明 |
|------|---------|------|
| `apps/library/mod.rs` | ~20 | pub use 声明 |
| `apps/library/api/mod.rs` | ~80 | Client struct + connect() + 2 端点 |
| `apps/library/api/loans.rs` | ~60 | loans + history 实现（若需拆分） |
| `apps/library/http.rs` | ~80 | build_client + fetch_json/fetch_html |
| `apps/library/models/mod.rs` | ~10 | pub use |
| `apps/library/models/loan.rs` | ~50 | LoanRecord + days_until_due |
| `apps/library/models/item.rs` | ~30 | BookItem（可选，T7 预留） |
| `apps/library/tests_parse.rs` | ~100 | mockito fixture 测试 |
| `commands/library/mod.rs` | ~15 | |
| `commands/library/handlers.rs` | ~60 | cmd_loans + cmd_history |
| `commands/library/data.rs` | ~40 | LoansData + HistoryData |
| **合计** | **~545 行** | 全部 < 200 行/文件 |

---

## 结论：4 个关键 unknowns（均需真机）

1. **入口域名**：`lib.sjtu.edu.cn` 下哪个子域 / 路径触发 CAS 登录
2. **进入方式**：CAS 302 链 还是 OAuth2 还是独立 session
3. **借阅列表端点**：路径 + 分页参数 + response 格式（JSON/HTML）
4. **字段名 + 日期格式**：`due_date` 是 ISO string 还是 Unix timestamp 还是中文格式
