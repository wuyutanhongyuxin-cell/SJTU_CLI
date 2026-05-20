# SJTU-CLI Lessons

> 自我改进循环日志。
> 每次被用户纠正、或踩到坑后，在此记录"错误模式 + 避免规则"。
> 会话开始时优先读一遍，防止同类错误重犯。

格式：
```
## YYYY-MM-DD — 简短标题
**触发情境**：什么时候发生
**错误模式**：我做错了什么
**正确做法**：以后应该怎么做
**规则**：一句话提炼成可执行规则
```

---

## 2026-05-20 — 本地 xray/v2ray 静默拦 lib.sjtu.edu.cn × my.sjtu app menu 是 L0 金矿 × chrome MCP 复用日常 profile

**触发情境**：S3 phase 2 library 子系统 L0 调研。chrome navigate `weijieyue.lib.sjtu.edu.cn:8080/wechat/sjtu/nowlend` 持续 503，连续三轮误判（"服务器挂"→"需 referer"→"必须经 my.sjtu SSO"→"假设 Primo SAML federation"），同样 URL `curl --noproxy "*"` 一次 200 OK + 完整 17KB HTML。最终发现根因是本地代理。绕大半天才回归"SJTU 自建简单 Servlet"真相。

**错误模式**：

1. **本地 v2rayN/xray HTTP proxy 对部分 .edu.cn 子域静默拦截返 503**
   - 监听 `127.0.0.1:10811` 走系统 HTTP proxy；流量先到 xray 才决策直连/出海
   - 不在 xray bypass 名单的 SJTU 子域 → 代理拒转发 → 返 503 + `content-length:0` + `proxy-connection:close`
   - 浏览器把这个 503 当 server 响应展示；判断信号是 response headers 里 `proxy-connection` 字段
   - 2026-04-23 lessons 已记过 reqwest 层 `.no_proxy()`，本条补充 chrome 浏览器层 + 真机 curl 层

2. **L0 直接从 SaaS deep link 入手，错过 SJTU 自家 app menu**
   - 看到 `86sjt-primo.hosted.exlibrisgroup.com.cn` 立即假设走 Primo PDS/SAML federation
   - 实际 `my.sjtu.edu.cn/api/task/me/apps` 一个 JSON 暴露全部 289 个 SJTU 服务的真实 URI（library 相关 25 条）
   - 借阅入口是 SJTU 自建 `weijieyue.lib.sjtu.edu.cn:8080/wechat/sjtu/*`，跟 Primo SaaS 完全无关
   - 浪费整一轮探不存在的 federation

3. **chrome-devtools MCP 复用日常 chrome profile（不是干净 incognito）**
   - 默认假设新干净实例，需手动登 jaccount
   - 实际 navigate `my.sjtu.edu.cn` 直接显示用户身份 + 已登录态
   - 走系统 HTTP proxy（注册表 ProxyEnable / ProxyServer / ProxyOverride）
   - `document.cookie` **不含 HttpOnly cookie**（如 JSESSIONID），但浏览器仍带它发请求 — 易误判"没 cookie"

**正确做法**：

1. 真机侦察"双轨对照"：chrome navigate + `curl --noproxy "*"` 同 URL 并行。chrome 失败 + curl 通 = proxy 拦截；两者皆通 = server OK；两者皆挂 = server/网络真挂
2. SJTU 子系统 L0 第一步**必走 my.sjtu app menu API**：
   ```js
   // 在 my.sjtu.edu.cn 已登录页面 evaluate_script:
   fetch('/api/task/me/apps', {credentials:'include'})
     .then(r => r.json())
     .then(d => d.entities.filter(e => /<keyword>/.test(e.name||e.nameEn||e.uri)))
   ```
   秒拿所有相关子系统真实 URI（含图书馆/邮箱/canvas/elec/jwc 等全部），效率比盲探 HTML 高一个数量级
3. chrome MCP 实例：① 默认带用户登录态直接可抓 ② 操作严格"只读访客"（不点 form/action 按钮，CLAUDE.md 硬红线）③ HttpOnly cookie 走 chrome MCP `Network.getCookies` DevTools API 而非 `document.cookie`
4. v2rayN 加 SJTU 新子域 bypass（双层）：① `guiNConfig.json` 的 `SystemProxyExceptions` 字段追加 `*.<域>`（持久化）② Windows 注册表 `HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings\ProxyOverride` 同步追加（立即生效）③ `InternetSetOption WININET_OPT_SETTINGS_CHANGED` 广播让 chrome 重读

**规则**：

- **R1**: 真机访问 SJTU 服务报 503 时，先 `curl --noproxy "*"` 对照。response headers `proxy-connection:close` + `content-length:0` 是本地代理拦截铁证（而非 server 故障）
- **R2**: SJTU 新子系统 L0 第一步**必查 my.sjtu `/api/task/me/apps` JSON**，grep keyword 拿真入口 URI 后才动手探 — 别从 SaaS deep link 倒推
- **R3**: chrome MCP **不是干净 incognito**，复用用户 profile：① 直接拿登录态（省登录）② 操作严格只读访客 ③ HttpOnly cookie 看不到要走 devtools `Network.getCookies` API
- **R4**: 项目首次接 SJTU 新子域被本地代理拦时，按 `*.<域>` 双层加入直连白名单（v2rayN SystemProxyExceptions + Windows ProxyOverride），保留 lessons 复用

---

## 2026-05-20 — SJTU 图书馆借阅子系统认证模型备忘（library 实装 reference）

**架构（项目内 reference，非错误）**：

- **入口**：`http://weijieyue.lib.sjtu.edu.cn:8080/wechat/sjtu/{nowlend,history,fine}` —— **HTTP 8080 plain，非 HTTPS！**
- **栈**：nginx/1.12.2 → Java Servlet（JSESSIONID `Path=/wechat/; HttpOnly`）→ DWR + jQuery + Mustache 渲染
- **SSO 入口**：`/wechat/sjtuAuth/oAuthSJTU?platform=phone&returnUrl=<encoded>` → jaccount OAuth flow → 回 returnUrl 时 JSESSIONID 已设
- **鉴权双层**：
  1. JSESSIONID HttpOnly cookie 承载真正 session（reqwest cookie jar 自动管）
  2. URL 参数 `session=<token>` 是**一次性 token**（每次 `GET /sjtuAuth/getSessionId` 返回新值，anti-replay 设计）
- **读 API**（只读，本项目可用）：
  - `GET /wechat/sjtuAuth/getPidFromSession` 检查登录态（result.result==1 即已登录）
  - `GET /wechat/sjtuAuth/getSessionId` 拿一次性 token（每次调返回新值）
  - `GET /wechat/sjtuAuth/getInfo?session=<sid>` 当前借阅 `{result, canRenew, borrowArray:[{isbn,bookName,author,loanDate,dueDate,currentFine,barcode,isReNew,isOverdue,isRecall}]}`
  - `GET /wechat/sjtuAuth/getHistoryBorrow?session=<sid>` 历史借阅 `{result, historyArray:[...]}`
  - `GET /wechat/sjtuAuth/getFineInfo?session=<sid>` 罚款 `{result, fineArray:[{isbn, status: '待缴纳'|'已支付'|'已免除', ...}]}`
- **红线写 API（永不实装，read-only 项目）**：
  - `/sjtuAuth/renew?session=&barcode=` 续借
  - `/sjtuAuth/generageDoPayData` 缴费下单（注意原拼写 typo "generage"，是 SJTU 服务端错拼，不要改）
  - `/sjtuAuth/updateCash` 更新缴费状态
  - `/sjtuAuth/checkIsPaid` 检查已缴费

**规则**：

- **R5**: library 模块实装：先 GET `/sjtu/nowlend` 触发 SSO 取 JSESSIONID → 每次查询前先 GET `/sjtuAuth/getSessionId` 拿一次性 token → 作 URL 参数 `session=<sid>` 传目标 API
- **R6**: reqwest 客户端**不要开 https_only**（library 入口是 HTTP 8080 plain text）；保留 cookie jar
- **R7**: 不要"修正" `generage` 拼写 — 是 SJTU 服务端原始 endpoint 名，改了 404

---

## 2026-05-20 — T7 R6 真机 CP：library production code 漏 `.no_proxy()`

**触发情境**：R1-R5 全绿（fmt + clippy + 340 测试全过）落地后，R6 真机 CP-L1 第一发就 503：
`/getPidFromSession status=503 Service Unavailable snippet=`。

**错误模式**：

- R2 写 mockito 测试时遇到 v2rayN 系统代理截 127.0.0.1 → 503，给 *测试用* 的 inline client 加了 `.no_proxy()`。
- 但 production `build_http_client`（src/apps/library/http.rs:47-68）**没加**。
- 在校园网内开发机（普遍开 v2rayN / Clash）跑 production code，:8080 端口流量仍被代理截走 → 503，跟测试期遇到的是同一根因，只是没在 production 复制 fix。
- elec / services / shuiyuan 等子系统不受影响是因为它们走标准 443 HTTPS，v2rayN routing 规则匹配 `*.sjtu.edu.cn` 直连；而 weijieyue 是 **HTTP plain text + 非标端口 8080**，routing 规则没命中。

**正确做法**：

- library production `build_http_client` 必须 `.no_proxy()`（src/apps/library/http.rs:60 加，注释 head 加说明）。
- 校园网内必须直连；校园网外无论代理与否都访问不到（内网 only）→ 强制 no_proxy 不会让任何场景变差。
- 真机 CP 验证：no_proxy fix 后三连全 200，count:0（用户从未借书）。
- OQ-LIB-1/5 无法回填（无书）；OQ-LIB-6 推论"空 fines 走 result:1 + fineArray:[]"（无错即对）；OQ-LIB-2/3/4 三连未触发 stale 路径。

**规则**：

- **R11**: SJTU 子系统涉及 **HTTP plain text** 或 **非标端口**（如 weijieyue:8080）时，production `build_http_client` 必须显式 `.no_proxy()`（v2rayN/Clash 截非 HTTPS 流量）。标准 443 HTTPS 不需要（系统代理 routing 自带 sjtu.edu.cn 直连规则）。
- **Why**: 中国大陆开发机普遍跑 v2rayN/Clash 系统代理，:8080 + plain HTTP 不在常规 bypass 名单内 → 截到代理 → 503。
- **How to apply**: 新接入 SJTU HTTP 子系统前先看 endpoint scheme/port。`http://*:non-443` → `.no_proxy()` 必加；`https://*:443` → 默认即可（依赖系统代理 routing）。

---

## 2026-05-20 — T7 library 实装：模仿 weixin path 范式 + 三层接线 + fixture-only 验证局限

**R8** library 子系统不接 CAS retry 层：weijieyue 走 jaccount OAuth dance（与 weixin path 同范式），
没有 CAS sub-session 概念，stale 直接抛 `SessionExpired` 提示重 sjtu login，不需要 cas 子系统的
SubSessionStale 信号。

**R9** HTTP 8080 plain text 子系统照常用 reqwest：scheme `http://`、port 8080 可正常注入 cookie。
reqwest 默认不强 HTTPS，无需 `.https_only(false)`。

**R10** mockito 不能伪造跨域 redirect：测 `SessionExpired on jaccount landing` 无法在 mockito 里
模拟（DNS 不解析 jaccount.sjtu.edu.cn）。两种兜底：① 单测层直接构造假 URL 走纯逻辑路径
② L5 真机 CP 故意 logout 验证。

**Why R8-R10：** 这些规则是 T7 plan 推导出的，写入 lessons 以便下一个 SJTU 子系统（图书馆 phase-2 /
邮箱 / 其它）能直接复用，不必每次反推。

**How to apply：** 新子系统接入时先决策：CAS（ASP 正方系）还是 jaccount OAuth（weijieyue / weixin
/ Canvas）？走 OAuth 路径就照 weixin / library 模式，不接 cas_retry 层。

---

## 2026-05-19 — reqwest 严格 URL parser × OAuth2 scope 空格 + cookie jar domain 分桶 + 主 session 被 `_` 标永久忽略（T4 weixin D12 三层 bug）

**触发情境**：T4 weixin path 上线后真机 4xx + parse 全 fail。surgical experiment 4 轮才锁定 3 层独立 bug 同时存在 + 1 处 parser 实站漂移。

**错误模式**：

1. **L3 reqwest URL parser 拒裸空格 Location**
   - SJTU PHP OAuth2 endpoint 302 Location 含 `scope=profile connect_wechat ...`（4 个空格分隔的 scope，未做 percent-encoding，违反 RFC 3986）
   - reqwest 严格 URL parser 拒整条 Location → redirect middleware **callback 零调用** + 直接把 302 当 final response 返
   - 浏览器宽容自动 fixup 把空格转 `%20`，让你以为 endpoint 正常
   - 判断信号：`Policy::custom` callback 一次都不触发 + 收到 3xx final response

2. **L2 reqwest cookie jar add_cookie_str 按 base URL domain matching**
   - 写法 `for c in &session.cookies { jar.add_cookie_str(&cookie_str, &weixin_url); }` 把所有 cookie 都用单一 base URL 注入
   - jar 按 RFC 6265 `host-matches` 检查：cookie domain `jaccount.sjtu.edu.cn` ≠ base URL `weixin.sjtu.edu.cn` → 静默拒绝
   - 结果：表面 jar 注入 6 cookie，实际只生效 weixin 域那 1-2 个；jaccount 域 JAAuthCookie 全丢
   - 没 error / 没 warn / 没 panic — silent rejection 是 reqwest cookie jar 默认行为

3. **L1 主 session 被 `_` 标永久忽略**
   - 函数签名 `fn fetch_balance(_main_session: &Session)` 表示"参数收下但不用"
   - 实际函数体里又调 `with_cas_refresh("card_weixin", ...)` 试图从 sub_session 拿 cookie — 但 weixin path 根本没有 sub_session 概念（直接走主 jaccount session）
   - Rust 编译器不报错（`_` 前缀就是告诉它别报 unused）— 100% 业务逻辑漏

4. **P1 parser 基于猜测的 HTML 结构**
   - plan 阶段假设 `<table><tr><th>字段名</th><td>值</td></tr>`
   - 实站：`<ul class="info-list">` + `<table class="table-condensed">` 缺 `<tr>` 包裹（连续裸 td）+ footer 用 colspan tr
   - 不真机 dump 永远不知道结构

**正确做法**：

1. 调试 reqwest redirect 行为：先 `Policy::none()` + 自己 GET + 把第一跳完整 response headers + body dump 出来，肉眼看 Location 真实形态是否符合 RFC 3986
2. cookie jar 用 `add_cookie_str(&str, &url)` 时 url 必须**跟 cookie domain 一致**，按 cookie 自身 domain 分桶；不存在"一个 base URL 注入所有 cookie"的合理写法
3. 函数参数 `_` 前缀是显式"这个参数永远不用"的契约 — 不要在函数体里又试图用它的同名值（即使是从别处拿）。要么改为 `main_session` 接受用法，要么真不用
4. parser fixture 必须用真机 dump 脱敏版而非 plan 阶段猜测；fixture 行数控制 30-60 行（节略多余 wrapper，保留 2-3 条样本）

**规则**：

- **R1**：调试 reqwest redirect 异常先 `Policy::none()` dump 第一跳 headers + body，验 Location 是否合 RFC 3986 严格语法
- **R2**：reqwest cookie jar 注入必须按 cookie 自身 domain 分桶（`for c in cookies { let url = format!("https://{host}/", host=c.domain.trim_start_matches('.')); jar.add_cookie_str(&set_str, &url); }`），不存在"一个 base URL 灌所有 cookie"的写法
- **R3**：`_` 前缀参数表示"参数收下但不用" — 函数体里再触碰这参数是契约违反；不要 `_main_session` + `with_cas_refresh` 这种隐式 fallback 链
- **R4**：实站 HTML 必须真机 dump 一次再写 selector — `<table>` 的 row 结构 / `<th>` 占位 / class 命名都不能假设。dump 完按 ~30-60 行节略 wrapper 当 fixture，selector 在 fixture 上 TDD

---

## 2026-05-16 — CAS retry 层 follow-up（T9 staleness 盲区根治 + T8 真机 CP-CR-1..3）

**触发情境**：T9 真机暴露的"jwc sub_session 客户端 fresh 但 ZF 服务端 timeout"盲区（lessons.md 2026-05-15 段 §4）。临时修复（手删 sub_sessions/jwc.json）不可持续，作 follow-up 系统化封装 retry 层。8-task TDD subagent-driven 实装（spec → plan → T1-T7 implementer + 两阶段 review + final review → T8 真机）。

### 真机 CP-CR-1..3 全过结果

- **CP-CR-1**（删 jwc.json → cache miss path）：19.3s pass。`sub_session 已落盘 name="jwc" cookie_count=4 elapsed_ms=12503` + envelope ok。`total_result=0` 是真实结果（2025 春季学期空），不是错。
- **CP-CR-2**（sub fresh 但 ZF 12 小时不动早 timeout → 触发 retry）：**第一跑 fail 暴露 T3 spec 盲区**（见下文 1），fix bind.rs 后第二跑 21.5s pass。retry warn 日志触发明确：`sub_session 服务端 stale，清缓存重做 CAS` + 重 cas_login 4 cookies + 二次 op 成功。
- **CP-CR-3**（手动改 JAAuthCookie=INVALID + 删 jwc.json）：4.0s fail，`子系统 'cas' 不可达：CAS 跳转最终停在 jaccount 域(...)。可能 JAAuthCookie 过期...请先 'sjtu logout && sjtu login'` 友好提示 + exit 1。`SubSystemUnreachable("cas", ...)` variant 正确（非 SubSessionStale，retry 不应试 — 主 session 挂了）。

### 关键设计教训

1. **T3 spec 漏覆盖：ZF stale 真实触发点是 pre-GET `visit_sp_page` 不是 POST `post_form_json`**（T8 暴露）
   - 原 T3 plan/spec 锁定 `src/apps/jwc/http.rs::post_once` 的 final_url detect，但实测 ZF 服务端 stale 时 **`bind.rs::visit_sp_page` 的 pre-GET 就被 redirect 到 `/xtgl/login_slogin.html`** 拦下，POST 根本走不到。我和 spec reviewer 都漏了这点。
   - Fix（commit 4d7f52d）：bind.rs:60-66 同域 redirect 路径改抛 SubSessionStale 替换 UpstreamError，retry helper downcast 命中。
   - **教训**：写 detect 类 spec 前必须 trace 调用链找"哪一跳最早触发"，不是看哪一跳"最有戏剧性"。POST 的 detect 漂亮但 ZF 实际拦在 GET。grep 整链路看 redirect detect 出现的所有位置（`grep -rn "login_slogin"` 一行命令本可在 spec 阶段发现）

2. **fail-soft 吃掉 retry 信号是 silent bug 高发区**（T6 ical/handler.rs fetch_all）
   - 老路径 `tokio::join!(...)` 后 `unwrap_or_else(|e| { warnings.push(format!("{e}")); default })` 把 SubSessionStale 错装成 warnings → retry helper 永远收不到信号 → T9 表面 envelope `ok=true` 实际 `eventCount=0`
   - Fix（T6 commit 24f875e）：fetch_all 改返 `Result<...>`，`join!` 后先 `for ... if let Some(SubSessionStale(name)) = err.downcast_ref()` 重 raise variant，stale 错跳过 fail-soft 直接上抛
   - 教训：任何 fail-soft 路径都必须先 detect "retry-able" 错误优先级，不能盲目吞错。fail-soft 接口的 retry-able 错处理是一类反模式

3. **anyhow + thiserror 混用时 downcast 链脆弱**（T7 invariant 守卫）
   - 反例：`anyhow!("{:#}", err)` 字符串重 raise 破坏 downcast 链 → retry helper `downcast_ref::<SjtuCliError>()` 拿不到 variant → 不 retry
   - 正确：`SubSessionStale(&'static str)` 是 `Copy`，pattern 拿出 `*name` 重新构造 variant `SjtuCliError::SubSessionStale(*name).into()` 重 raise
   - tests/cas_retry_signal.rs 加 3 cross-module sanity tests 当 invariant 守卫（正例 boxing + 正例 context wrapping + 反例 string reraise 必须破坏 downcast）

4. **手卷 vs middleware trade-off 进 retry.rs module doc**
   - 2026 业界 idiomatic 是 reqwest-middleware + RetryableStrategy，CLAUDE.md 不引新依赖 + 改造面 ×6 子系统 + stateful side-effect（clear_sub_session）+ 1 子系统 scope = 手卷 4 条理由写进 `src/auth/cas/retry.rs` 顶部
   - 未来扩到 4+ 子系统再 reconsider middleware

5. **同构 pattern 先例复用 = 设计成本几乎 0**
   - `canvas_video/retry.rs::with_token_refresh`（49 行 production 验证）→ `cas/retry.rs::with_cas_refresh`（51 行）直接同构。改进：抽 `with_refresh_inner(initial_session, op, refresh)` 注入 refresh fn → 单测不依赖文件系统（T4 3 测全用 inject mock session）
   - 教训：codebase 内同构先例胜过外部业界 best practice；先 grep 自己再 google

### 真机新发现（暂不修，记为 follow-up）

6. **`cas_login` follow_redirect_chain 偶发性 partial cookie**（T8 CP-CR-2 第二跑暴露）
   - 同样 JAAuthCookie + 同样 LOGIN_URL，相邻几分钟内两次 cas_login 拿到 cookie 数不同：第一次 3 个（缺 i.sjtu.edu.cn JSESSIONID）→ ZF 不认 → 第二次还 stale → retry 失败；4 分钟后再跑 cas_login 拿到 4 个 cookie → ZF 认 → 成功
   - 根因猜测：ZF 短时间内重做 CAS 给 partial response（rate limit / 异常缓存 / SSO server 状态机），cas_login `follow_redirect_chain` 跳数走完但中间某跳没给 Set-Cookie
   - Follow-up（不阻塞）：cas_login 加 cookie 数 sanity（"主域 i.sjtu.edu.cn 缺 JSESSIONID 时 warn + 一次重试"）；或 retry helper 在 second op 失败时尝试 third（指数退避 2-3s）
   - 教训：CAS 链路非幂等。retry helper 假设"refresh 后必拿到 fresh session"在 ZF 上偶发不成立

### 设计决策（追加）

- **SubSessionStale variant 比字符串匹配胜出**：强类型 retry pattern，不依赖错误 message 文案。`code() = "session_expired"` 复用 envelope code 不引新值
- **retry 闭包接 Session 不接 Client**：cookie jar 必须重 build_http_client，无法复用旧 Client；闭包内 `Client::from_session(session)?` + `Fn` 多次调用 + 双 clone outer/inner 模式
- **`with_refresh_inner` 抽出 refresh fn 注入**：单测不需要 mock cas_login（pure 文件 IO），core retry logic 独立可测；ical/handler.rs::fetch_all 复用此模式守 stale 信号优先级

### 接入范围与 follow-up

- **本轮接入**：jwc 9 个 call site（cmd_grades / cmd_schedule / cmd_gpa / cmd_gpa_by_semester / cmd_exams / cmd_today / cmd_week / cmd_next / run_calendar）+ ical/handler.rs::fetch_all 修 fail-soft 吃信号 bug
- **未接入子系统**（spec NG1）：elec / services / jwbmessage 暂不动 — 真机未暴露同类 staleness，且需各自 SP 的 stale detect 信号调研（Discourse OAuth2 / canvas LTI 的 stale 信号不同）
- **零余量文件 4 个**（final reviewer flag）：`schedule_handlers.rs` 200/200 / `cas/mod.rs` 199 / `ical/handler.rs` 196 / `cli/jwc/mod.rs` 195。下次该文件有任何改动前先拆

---

## 2026-05-09 — 批量下载先验产物再调 API：临用临取 + 断点续传两条规则（CP-V4）

**触发情境**：CP-V4 设计 `sjtu canvas-video download --lectures all` 18 讲 × 2 机位 = 36 个 mp4。两条已知约束相互打架：① mp4 URL 含 `key=` 时效签名 1-3h 过期 → 不能开局一次性 batch-fetch 36 个 URL；② 36 文件 ~30+ GB 任一中途失败若不能续跑，下次得重抓。需要既"临用临取"又"已下不重下"。

**错误模式（设计期就能预见的）**：
1. 把"先调 get_video_info 拿 URL"作为唯一入口 → 每次重跑都会无谓重发一次拿 URL，浪费 1-2s 网络往返 × 18 讲 = 30s+；更坏的是若网络抖动这步先挂，本可 skip 的讲也被打断
2. 用文件大小哈希校验做 skip 判断 → 没有官方 size 接口，调研里 v.sjtu 也没返 Content-Length 可信值；只能用 size>0 + 路径模式的"曾经下完过"启发式
3. fail-soft 与 skip 状态混在一个布尔里 → 调用方没法分辨"上次已下完"vs"这次刚成功"vs"这次失败"。Envelope 里 `status: ok / skipped / partial / failed` 四态分清才好下次操作

**正确做法**（已落 CP-V4 实装）：
1. **先文件后 API**：`check_skip(target, channel, args)` 用 `safe_filename` 重建 dest 路径，`std::fs::metadata` 查在不在 → 若 audio_only 看 m4a / 否则看 mp4 → size>0 即认为是上次成功产物，构造 ChannelOutput 直接进 entry，**跳过 get_video_info 整段**
2. **临用临取在 download_one_channel 一线**：所有非 skip 路径在该函数内才调 `client.get_video_info(...)` 拿当下新签的 URL，即使前 17 讲下完用了 2 小时，第 18 讲签名仍是当前发的不会过期
3. **Envelope 四态**：`derive_status(want, got, errs, all_skipped)`：want=channels.len()，got=成功记录数，errs=失败记录数；`all_skipped && got==want → skipped`、`errs.is_empty() && got==want → ok`、`got.is_empty() → failed`、其他 → `partial`
4. **stderr 进度不污染 stdout envelope**：`eprintln!("[{i}/{n}] 第 {seq} 讲 ...")` + `"  ⚠ {msg}"` + `"  ✓ ch{c} 已存在 ({size}) → skip"`；envelope 走 stdout 的 yaml/json，AI agent 可双向消费

**规则**（按"实装时一行 grep"标准写）：
- ✅ **批量下载先验本地产物再调远端 API**：把"已下完产物的探测"作为循环顶层的第一步，避免无谓 token 消耗 + 让本能 skip 的迭代不被入口 API 抖动打断。Why：批量任务最贵的是端到端时间，最容易翻车的是中途网络。How to apply：所有 batch downloader（不仅 canvas-video）必须先 file-meta probe 再 fetch metadata
- ✅ **签名带 TTL 的下载链严禁 batch pre-fetch**：API 拿 URL 后必须立即开下，不能存到下游再用。Why：预拿到的过期 URL 全是垃圾，且 fail 时分不清是网络挂还是签名挂。How to apply：循环里 `let fetch = ...await?; let bytes = download(&fetch.url)...` 紧贴写，中间不要插 sleep/agg/sort 等动作
- ✅ **批量结果用四态 status 而非二态 success bool**：`ok` / `skipped` / `partial` / `failed` 四态，且 `partial` 留给"双机位时一路成功一路挂"这种半成品。Why：用户复跑时能精确选 partial+failed 的子集补，bool 只能全跑。How to apply：CLI 批处理 envelope 一律 status: enum-string，不要 bool

**真机验证**：
- audio-only 单讲：840MB mp4 → 20MB AAC m4a 共 105s（mp4 84s + ffmpeg 21s），ffprobe 验 codec=aac
- 断点续传：同条件二跑 status=skipped、succeeded=0、skipped=1、bytes=磁盘 size、elapsed_ms=0（仅 LTI launch 21s）
- 进度行验证 stderr 不进 envelope 流：`./sjtu ... --json | jq` 直接通

---

## 2026-05-08 — reqwest 默认 H2 让多段 Range 复用单 TCP，被 CDN 按 per-conn 限速（CP-V3.1 加速 4×）

**触发情境**：CP-V3 真机 800MB mp4 下载耗 800s（~1MB/s），怀疑是 SJTU CDN 总带宽限制。联网调研发现 reqwest #976 / uv #17204 都是同一个症状：**reqwest 默认 ALPN 协商到 HTTP/2，N 段并发请求在底层 multiplex 到一条 TCP 连接**，被 CDN 按 per-connection 整体限速。真机实证：`.http1_only()` + `.pool_max_idle_per_host(0)` 强制每段独立 TCP 后，800,483ms → 201,946ms（**3.97× 提速**），无新依赖、无新段池算法、download.rs 改 ~5 行。

**错误模式**：默认相信 reqwest "并发等于多连接"。N 段 spawn + N 个 cli.clone() 看起来像 N 条独立连接，但 reqwest 内部连接池 + H2 multiplexing 会把它们折叠回一条。CDN 限速维度若是 per-TCP-connection（业界常见），并发数变化对吞吐零影响。

**正确做法**：下载场景的 reqwest Client 必须显式：
1. `.http1_only()` —— 关 H2 ALPN 协商，强制 H1.1
2. `.pool_max_idle_per_host(0)` —— 关 idle 连接池，每个 send() 都建新 TCP
3. `.tcp_nodelay(true)` —— 防 Nagle 算法粘连小包（reqwest 默认就开，显式写出来锁定意图）
段间 spawn 错峰几十 ms（让 CDN 看到的 SYN 间隔不是同瞬抵达）防 burst 触发限流。

**对照参考**：`prcwcy/sjtu-canvas-video-download`（Python+aria2，同 SJTU CDN）用 `aria2c -x 16` 跑通；aria2 默认就是每段独立 TCP，且 H1.1 only。我们用 reqwest 跑 H1.1 + 关池 = 等效路径。

**规则**：
- **下载类 reqwest Client 永远显式 `.http1_only().pool_max_idle_per_host(0)`**。Why：默认 H2 复用让"并发=同 TCP 多 stream"被 per-conn 限速；这是 reqwest #976 等长期未关 issue，不是我们项目特有 bug。How to apply：`Client::builder` 用于 Range 分片 / 多文件并发下载时必加，普通 API 调用不必。
- **遇到"加并发不提速"反射弧先看 HTTP 协议层不是看段数**。Why：HTTP/2 multiplexing 是性能反模式在限速 CDN 场景。How to apply：吞吐打不上去时，先 `curl --http1.1 -H 'Range: bytes=0-1000000' URL -o /dev/null` 单段看真实速率，对比 reqwest 行为。
- **联网调研 root cause 比上段池/动态切片划算**。Why：方案 1 改 5 行拿 4× 收益，方案 2（aria2 SegmentMan 段池）要 80 行；先最便宜的加速点榨干。How to apply：性能问题先 web search "<工具> <症状> issue"，看上游有没有同类 bug 的标准解，别上来就自己造段池。

---

## 2026-05-08 — SJTU 教学 CDN 多段并发 Range 下载触发 504：8 路过载，4 路才稳（CP-V3 真机）

**触发情境**：CP-V3 实装 mp4 Range 分片并发下载（`apps/canvas_video/download.rs`），按调研 §7 "MVP 8 段并发"建议默认 `--concurrency 8`。真机 800MB mp4 第一跑：段 0/1 飞快下完，段 2-7 全部返 `504 Gateway Timeout`，linear backoff 500ms × 3 次重试都被拒，整体失败。

**错误模式**：
1. **盲信调研建议值**：调研报告写"MVP N=8"是参照 SJTU-Canvas-Helper（GUI 工具，user 主动盯）。CLI 自动化场景下 CDN 限流后 user 不在场及时退避，8 路同时打 = 7+ 段同时被拒
2. **backoff 太短**：500ms / 1000ms 梯度对 5xx 完全不够 —— 504 通常意味着上游 origin 处理慢（不是 transient 网络抖动），毫秒级重试只是再戳一次刚被打回来的同一座墙
3. **没分清 4xx vs 5xx**：网络重试策略用 `is_retriable` 一刀切（含 5xx），但 5xx 里 502/504（gateway）和 503（service unavailable）都是上游需要更长喘息，跟 timeout/connection reset 这种瞬态错严格不同档次

**正确做法**（已落入 CP-V3 实装）：
1. **CLI 默认值往保守调**：`--concurrency` 默认 4 而非 8。调研 MVP 假设值放 doc，CLI default 走"对方服务器友好"
2. **梯度 backoff 而非 linear**：`[0, 3000, 10000, 25000]ms` 四档，给 CDN 起码 30+s 总缓冲。最末段单次 attempt 可能慢，但能救活整个下载（实测段 3 attempt 2 救活）
3. **看末段是否单点慢**：真机观察段 0/1/2 一次过、段 3（最末段 630M-840M）反复 504。可能是 CDN 对文件 trailer / EOF 区域有特殊处理或源站慢盘读。CP-V4 批量场景如果还遇到，可以考虑"反序下载"或"末段单独单线程"

**规则**：
- **下载类调研报告里的并发数 / chunk 大小，CLI default 直接砍半**：调研用 GUI 工具（用户在场）的经验值，CLI 走保守。**Why**：CLI 自动化失败时用户不一定立刻看到，对方限流陷阱里要友好退避。**How to apply**：任何 CDN / 大文件 / 批量并发任务，看到调研建议 N，CLI default 设 N/2 起步
- **5xx 重试 backoff 梯度走"秒级"不是"毫秒级"**：504/502/503 是上游需要喘息，不是重发就好。**Why**：网络层瞬态错（TCP reset / timeout）毫秒级 retry 有意义，gateway/upstream 5xx 毫秒级 retry 等于戳同一道墙。**How to apply**：retry backoff 至少 `[0, 3s, 10s, 25s]` 梯度，总缓冲 ≥ 30s
- **保 .part 临时文件原子合并**：分片成功后写 `<dest>.tmp` 再 `rename` → `<dest>`；中途失败保 `.part{i}`（File::create 已 truncate，retry 自然覆盖）。**Why**：部分写文件直接落最终路径会让"文件存在"和"文件完整"语义混淆；调用者看到 `dest` 文件就以为下完。**How to apply**：所有大文件下载 / 数据导出，必须 `tmp`+`rename` 原子化，绝不直接写最终路径

> **2026-05-08 retrospective**：上面"4 段才稳"的定论建立在"reqwest 默认 H2 让 N 段共用一条 TCP"的隐含前提上。CP-V3.1 切到 H1.1 + 关池后每段独立 TCP，8 段 / 16 段都不再触发 504。本条规则"调研建议值砍半"在该前提变更后不再适用 —— 见上一条"per-conn 限速"教训。

---

## 2026-05-08 — worktree 隔离 subagent 看到的是 base commit，不是主分支 HEAD（CP-V1 合并坑）

**触发情境**：CP-V1 编码 delegate 给 subagent，开 `isolation: worktree`。subagent 跑完四关全绿交付，但合并回主仓库前 review 发现 worktree 的 `src/cli/mod.rs` 把 `mod elec` / `mod jwc` / `mod services` 三个声明 + Commands enum 三个 variant + dispatch 三个 arm **全删了**。差点直接 `cp` 整文件回主仓库覆盖前面 793915c + 947ce6f 两个 commit 的实装。

**错误模式**：默认 worktree 是从主分支 HEAD 拉的。实际 `git worktree add` 拉的是 **创建 worktree 那一刻** 的 commit；如果中间主分支又往前 commit 了几个，worktree 基底就落后。subagent 看到的 `cli/mod.rs` 是老版本（没有 elec/jwc/services），它老老实实在老版上加 `CanvasVideo`，结果产生的 diff 长得像"删了 3 个 variant + 加了 1 个 variant"。如果不细看就 `Copy-Item -Force`，主分支历史就被静默回滚。

**正确做法**：
1. **派 subagent 走 worktree 前**：先 `git -C <worktree> log --oneline -1` 看 base commit，对比主仓库 `git log -1`，落后了就 worktree 内 `git pull` 或重建 worktree
2. **subagent 交付后合并**：永远先 `git -C <worktree> diff --stat HEAD` 看改动行数。若 mod.rs / lib.rs 类聚合文件出现 `-N +M` 而 N 异常大（>追加行数），警铃响 —— 大概率 base 落后导致看到老版本
3. **合并策略**：只 cp 新建文件（`?? src/...`）；对 mod.rs / lib.rs / Cargo.toml 这类聚合文件，永远 **手工 Edit 追加 canvas_video 相关 lines**，不整文件覆盖
4. **派 subagent 时主仓库尽量干净**：若主仓库还有 staged 改动，subagent worktree 一律看不到 —— 要么先 commit 再开 worktree，要么 `git stash` 后再开

**规则**：worktree 是 base commit 的快照，不是 HEAD 的实时镜像；合并 subagent worktree 改动时只 cp 新建文件，聚合 mod 文件一律手工追加。

---

## 2026-05-08 — Canvas (oc.sjtu) SSO 触发不在纯 302 链上，要点击或找直跳 URL（CP-V2 真机阻塞）

**触发情境**：CP-V1 编码完单测全绿，跑 `sjtu canvas-video list 88168` 真机 30s 超时。逐层诊断：先发现 chrome 端注入 cookie 全被 `domain==""` 过滤；加 `cas_login("canvas_oc", oc URL)` 给 oc 域签 session；cas 模块跟下来的 cookie domain 全空 + 末跳 200 HTML 停 —— hop 0 oc/courses/.../external_tools/8329 → 302 → hop 1 oc/login → 302 → hop 2 oc/login/canvas → **200 HTML 终止**。整条链没碰过 jaccount，落盘 5 个 anonymous cookie，chrome 注入后 navigate 仍被踢回同一个 /login/canvas。

**错误模式**：默认所有 SJTU SP 的 SSO 形态一致（jwbmessage / elec / jwc/ZF 都是 302 链 follow 完即拿到认证 session）。把这个假设套到 Canvas 上设计 CP-V1：用 cas_login 给 oc 拿 cookie + 用 headless chrome 接力 LTI launch。事实 SJTU Canvas 的 SSO 触发是 **浏览器端 form/JS-driven**：`/login/canvas` 是登录方式选择页（静态 HTML），有"Sign in with JAccount"按钮，点了才 form-submit 到真正的 OAuth 入口（推测在 `/login/oauth2_provider/...` 之类）→ 302 jaccount → SSO → 302 oc/oauth2_callback → 种认证 cookie。reqwest 不跑 JS、不会点按钮；headless chrome `navigate_to` 也只 navigate 不点按钮，`wait_until_navigated` 一返回就 done，根本不到 SSO 那步。

调研漏斗：CP-V0 调研期是用户自己在浏览器**已登录**会话里 LTI launch（cookie 全在），抓的 network 是从 external_tools → form_post 到 v.sjtu 这一段，**完全跳过了 oc 自身的 SSO 触发段**。所以原 `canvas_video_investigation.md` §2.5 第 4 点说"CLI 复用 cas_login 拿 oc cookie"在原理上写对了但 URL 没指明 —— 只要把它落到代码就发现 cas_login 拿的不是认证 cookie。

**正确做法**：
1. **调研期必须用 incognito + 未登录浏览器** 触发 SSO 一次 —— 真实跳转链才完整暴露。已登录会话只能验证下游 API，不能验证 SSO 入口
2. **每个 SP 调研第一步**：用 chrome-devtools MCP `list_network_requests` 在 navigate-to-SP 后看：①前 5 跳是不是全 302；②有没有 jaccount 域出现。两条都是 → 正常 302 链可以 cas_login。任一缺失 → 要么找直跳 SSO URL，要么用 headless 模拟点击/form-submit
3. **Canvas 类 button-driven SSO** 修法两条路：
   - A：找出"Sign in with JAccount"按钮对应的真实 form action / URL（如 `/login/oauth2_provider/sjtu` 之类），cas target 改成它
   - B：headless chrome 在 navigate 后用 evaluate_script 定位按钮节点 click()，等下一波 navigate 完成
4. **域 cookie 兜底**（已落 auth_chrome.rs build_cookie_params）：cas 模块 `follow_redirect_chain` 收 Set-Cookie 时未按 RFC 6265 §5.3 默认填 request URI host —— 落盘 cookie domain 字段常为空，注入到 chrome 时被过滤。S2 修通用 bug 后此 fallback 可删

**修复结论 (同日 chrome-devtools MCP 协议超时 → 改纯 curl 调研推翻原假设)**：
"button-driven SSO" 假设错了。`/login/canvas` 静态页里 `<a href="/login/openid_connect"><div id="jaccount">…</div></a>` —— **"Sign in with JAccount"按钮其实是普通 `<a>` 超链接**，点击 = 普通 GET，不是 form-submit、不是 JS。`/login/openid_connect` 直接 302 → `jaccount.sjtu.edu.cn/oauth2/authorize?client_id=lACSIkmjF7lRHNKaVrIp&...`（OIDC Authorization Code Flow），cas_login 既有 302 跟链逻辑直接能跑通。

oc.sjtu (Canvas) **与 N305005 ZF 套路完全同构** —— 见下面 "## 2026-04-26 — i.sjtu CAS 入口是 `/jaccountlogin`" 那条：都是"SP 内部 login 页 HTML 里有 jAccount 锚点，那个锚点的 href 才是真 CAS 入口"。ZF: `<a href="/jaccountlogin" id="authJwglxtLoginURL">`；Canvas: `<a href="/login/openid_connect"><div id="jaccount">`。CP-V1 设计 `cas_target` 时已经知道 ZF 这条规则，但偷懒拍了个 LTI launch URL 就上线了，没去查 oc 自己的 login 页 —— **同一类错误第二次犯**。

修复就一行：`cas_target = "https://oc.sjtu.edu.cn/login/openid_connect"`。CP-V2 真机 18 讲返回正常，cas 首跑 10.5s → 缓存命中 7ms。

**规则（替代原"button-driven"路线）**：
- ✅ **每个未知 SP 调研第一步**：`curl` 它的根页 / login 页 HTML，`grep -i "jaccount|oauth|openid|saml" | grep '<a '` 找带 jAccount 字样的 `<a href>` —— 那个 href 就是 CAS 入口候选。10 秒搞定，比 chrome-devtools MCP 稳得多
- ✅ 候选 URL 拿到后 `curl --max-redirs 0 -i` 验单跳：返 302 + Location 含 `jaccount.sjtu.edu.cn/oauth2/authorize` = 命中 OIDC 入口；返 200 HTML = 还是个静态页，再往里找一层
- ✅ chrome-devtools MCP 不可用时（如本次 `Network.enable` timeout），纯 curl 完全够用 —— 调研只读探测不依赖浏览器
- ❌ 不要假设 SP 把 LTI launch / 深页直接当 SSO 触发器 —— 大部分 SP 把"未登录 → 自家 login 页 200 HTML"作为入口，要从 HTML 里找锚点
- ❌ 不要拿"用户已登录会话抓的 network"当 SSO 调研依据 —— 那只验证下游 API，跳过了入口

**Why 写这条**：CP-V1 设计时把"ZF 入口要找 HTML 锚点"当成 ZF 独有特性，实际是 SJTU 多个 SP 共用模式。先写规则再实装，能省一次 30s timeout + 一次 commit 回滚。
**How to apply**：以后接入新 SP（library / canvas / oc / 任何带"内部登录方式选择页"的系统），先 curl + grep 锚点，再写 `cas_target`。

---

## 2026-04-26 — i.sjtu CAS 入口是 `/jaccountlogin`，不是 nav 深页（CP-J1 实装坑）

**触发情境**：S3f 实装 N305005 学生成绩查询 CLI，照 `cas/mod.rs` 注释"target_url 必须是 SP 真正进的页面（如 `/xtgl/index_initMenu.html`）"把 jwc 的 LOGIN_URL 设成深页。CAS 链跑完落盘 sub_session（JSESSIONID + keepalive 两条 cookie），但调 `POST /cjcx/cjcx_cxXsgrcj.html?doType=query&gnmkdm=N305005` 一律收 ZF 自定义 `status=901` + 空 body。

**错误模式**：把"target_url 给 SP 深页"当成普适规则套到 i.sjtu 上。实际 i.sjtu（ZF）和 my.sjtu（jwbmessage）的 SSO 形态根本不同：
- my.sjtu / shuiyuan：直接 GET SP 深页 → 自动 302 到 jaccount → JAAuthCookie 验过 → 跳回 SP，整条链 4-6 跳走完，cookie 落盘
- i.sjtu / ZF：直接 GET 深页（甚至 nav 主页 `/xtgl/index_initMenu.html?jsdm=xs`）**只 2 跳**就停在 ZF 自家内部 login 页 `/xtgl/login_slogin.html`，根本没去 jaccount —— 落盘的是 anonymous JSESSIONID，server 端没绑 user_id

ZF 的 OAuth2 入口必须显式触发：从 login 页 HTML 里能看到 `<a href="/jaccountlogin" id="authJwglxtLoginURL">通过jAccount登录</a>`，**这才是 CAS 入口**。访问 `/jaccountlogin` 才会触发 8 跳完整链路：i.sjtu → jaccount/oauth2/authorize?client_id=MVJGw8u0bzoMJVbfb4Fk&redirect_uri=... → jaccount/jaccount/jalogin?sid=jaoauth220160718 → JAAuthCookie 验 → jaccount/oauth2/authorize?context=...&jatkt=... → http://i.sjtu.edu.cn/jaccountlogin?code=... → https 升级 → /xtgl/login_slogin.html（server 处理 OAuth code）→ /xtgl/index_initMenu.html?jsdm=xs&_t=...&echarts=1（200 终点）。落盘 4 cookies（JSESSIONID + keepalive + 2 个 path 维度变体）。

第二个 ZF 独有坑：**首次数据 POST 之前必须先 GET SP 页面一次**（`/cjcx/cjcx_cxDgXscj.html?gnmkdm=N305005&layout=default`），ZF server 才会把 gnmkdm 绑到 Tomcat session，后续 POST 才会被认。否则就算 session 是认证过的也照样 901。浏览器里的"点 nav → 进 SP 页 → 按查询"流程隐含了这步 GET，CLI 必须显式补。

诊断这个 901 的关键：通配的 final_url 检查不够，必须**对比 final_url 的 path 与请求 path 是否一致**——ZF 内部 login 页 (`/xtgl/login_slogin.html`) 落在 `i.sjtu.edu.cn` 同域，单看 host 防不住。

**正确做法**：
- ZF 实例（i.sjtu / 教务）的 CAS LOGIN_URL = `https://i.sjtu.edu.cn/jaccountlogin`，不是 nav 深页
- 任何 SP 数据 POST 之前先 GET 该 SP 页面 = "register" gnmkdm 到 Tomcat session（同 Client 生命周期内每个 page_path 只 GET 一次，用 `Mutex<HashSet<&'static str>>` 缓存）
- pre-GET 必须比对 `resp.url().path()` 与请求 path；不等 → 主动报错 `session 在 ZF 侧未认证`
- 调试 ZF 链路：`RUST_LOG="sjtu_cli::auth::cas=debug,jwc=debug"`，每跳 URL 都打出来，看落点是不是真的进了 SP 页

**规则**：
- ✅ 每个 SP 第一次开实装 → **先开 RUST_LOG=debug 数 hop 数**：少于 5 跳 + 没经过 jaccount = CAS 入口选错
- ✅ ZF 实例的 CAS 入口从其内部 login 页 HTML 找 `id="authJwglxtLoginURL"` / `href="/jaccountlogin"` 这种锚点；my.sjtu 等 OAuth2-direct 实例不需要
- ✅ 数据 POST 前 pre-GET SP 页 + final_url path 严格匹配；同 Client 内 cache 已绑过的 SP 集合
- ❌ 不要把 jwbmessage / shuiyuan 的"target_url = SP 深页"硬套到 ZF
- ❌ 不要单凭 `final_url 不在 jaccount 域` 就当作 CAS 成功 —— ZF 内部 login 页同域，需要 path 匹配
- ❌ 不要看到 `status=901 空 body` 就以为是 cookie 注入问题；先排 SP 模块未绑 / session anonymous 这两条

---

## 2026-04-26 — i.sjtu = ZF 教务系统 + 半自动 chrome-devtools 调研范式

**触发情境**：用户说"i.sjtu.edu.cn 完整严格详细准确实现"，我下意识把 i.sjtu 当"交我办"聚合门户去规划 SP 跳板调研（C 选项）。chrome-devtools `take_snapshot` 一抓页面 title="教学管理信息服务平台"，nav 全是教务向（报名/选课/成绩/课表/评价），**根本不是聚合门户**——i.sjtu 是 ZFSOFT 正方教务系统的 SJTU 实例（server header `ZFSOFT.Inc + Tomcat 7.0.94 + Java 1.8`）。聚合门户其实是 my.sjtu.edu.cn。继续抓 N305005 学生成绩查询：页面 GET 返 HTML 含 form，"查询"按钮 POST `cjcx_cxXsgrcj.html?doType=query&gnmkdm=N305005`，form 含 `xnm/xqm/queryModel.showCount/queryModel.currentPage/time` 等字段，response 是统一分页 envelope `{currentPage, totalCount, totalResult, items:[...]}`，每条 item 50+ 字段含大量内部冗余（`queryModel` 嵌套自己一份、`date/dateDigit` 响应时间、`xh_id` 256-hex token、`userModel` 空对象等）。用户红线：选课/信息维护/教学评价/报名申请/任何 form submit 全禁；用户偏好"我抓只读、你点查询/写"半自动模式。

**错误模式**：
1. 没先 take_snapshot 确认 i.sjtu 实际身份就开始规划聚合门户调研 —— 下意识按"i.sjtu 听起来像 portal"假设走
2. 把 ZF 系统的 GET-via-POST 模式与"chrome-devtools 任何 click 都是写"混为一谈，没意识到「查询」按钮虽然物理 POST 但语义只读
3. 看到 response 50+ 字段直接想全暴露给 CLI —— 实际 ZF 把内部字典/分页 envelope/render hint 全塞回来了，CLI 模型只该取 ~15 个核心业务字段

**正确做法**：
1. SJTU 任何子域调研第一步 `take_snapshot` 确认 title / 顶部 nav 实际定位，再决定调研策略；URL 名 ≠ 系统身份
2. ZF 系统调研 SOP：navigate_page 到 SP 页 → 抓 form 结构 → **请用户点查询按钮** → list_network_requests 抓 `doType=query` POST → get_network_request 拿 form body + response shape → 归档进 `tasks/isjtu_investigation.md`
3. 字段筛选：`item` 里只挑业务字段（学年/学期/课程/学分/成绩/教师等）+ ZF 内部 ID 仅作为 join key 内部用、`queryModel`/`userModel`/日期冗余/`localeKey`/`row_id` 全丢；`xh_id` 256-hex 不是真学号是签名 token，**不要落日志**

**规则**：
- ✅ i.sjtu = ZF 教务（不是交我办聚合门户）；交我办 = my.sjtu；CLI 实现规划在 `apps::jwc/`
- ✅ ZF 全 SP 走 `https://i.sjtu.edu.cn<path>?gnmkdm=<gnmkdm>&layout=default` 模板，数据接口走 `<page>?doType=query&gnmkdm=<gnmkdm>`，POST + form-urlencoded
- ✅ 所有 ZF 数据响应都是 `{currentPage, pageNo, pageSize, totalCount, totalPage, totalResult, items}` 分页 envelope；CLI 抽一个 `JwcPage<T>` 统一 deserialize
- ✅ ZF 必带 headers：`X-Requested-With: XMLHttpRequest` + `Accept: application/json, text/javascript, */*; q=0.01` + `Origin/Referer/UA`；缺 X-Requested-With 会被路由到 HTML 兜底
- ✅ ZF cookie：`JSESSIONID`（HttpOnly）+ `keepalive`（响应自动刷）；reqwest cookie store 自动接住
- ✅ ZF csrf：在 page HTML `<input type=hidden name=csrftoken>`，**不在 cookie**；写操作再去 parse，读操作不需要
- ✅ chrome-devtools 调研 i.sjtu / 交我办时严守半自动：snapshot/network/只读 evaluate 我做，任何 click / submit 用户做（feedback_isjtu_semiauto.md）
- ❌ 不要把聚合门户的 SP-jump 假设套到 i.sjtu —— i.sjtu 是单系统、有自己的 nav，不需要 jaccount-jump 逐 SP 兑 cookie
- ❌ 不要 force parse `cj`/`bfzcj` 成数字；考核类课程会给"通过"/字母等级
- ❌ 不要把 `totalResult`/`xf` 当 int —— ZF 序列化全是字符串，要么 String 要么自定义 deserialize
- ❌ 不要在归档 / 日志 / 提交里留任何真实学号 / 姓名 / 成绩值；规格表只写字段定义和接口形态

**当前代码状态**：
- ✅ 9 SP 规格已全部归档 `tasks/isjtu_investigation.md` §2.1–§2.9（成绩 / 课表 / GPA / 考试 / 成绩明细 / 修业情况 / 周课表 / 培养计划 / 毕业设计）
- ⏳ `apps::jwc/` CLI 实现未起；起手时第一件事是抽 `JwcPage<T>` + ZF 共用 client（headers / cookie / referer 模板）

---

## 2026-04-26 — ZF 教务 9 SP 调研挖出的 API 形态坑（实装速查）

**触发情境**：调研完 i.sjtu 9 SP 后准备开 `apps::jwc/` MVP。9 个端点里有 6 个不是"标准单 POST 拿 items"模式，提前不归档下次实装时很容易按 N305005 范式硬套结果 4xx / 数据空 / 全校扫描。

**坑位速查（按 SP 排序，全部已落 `tasks/isjtu_investigation.md` §2.x，本表仅作 grep 入口）**：

| SP / 功能 | 偏离点 | 不知道会出的事 |
|---|---|---|
| **N309131 GPA**（§2.3）| **两阶段调用**：先 `POST tjGpapmtj` 触发统计（返字符串 `"统计成功！"`，不是 JSON 对象），再 `POST cxGpaxjfcxIndex?doType=query` 拿数据 | 直接打第二个端点拿到的是上一次/空统计；第一阶段 response 用 `serde_json::Value` 接，不要预期 envelope |
| **N358105 考试**（§2.4）| 主键 button id = `btn_search`，触发 url 含 `?su=<学号>` query | 点 `search_go` 拿不到东西；form body 里没学号字段，全在 URL |
| **N305007 成绩明细**（§2.5）| **Master-detail**：`cxXsKcList` 主表 + `cxXsKccjList` 详表，`jxb_id` 串联；详表 item 有 `xmblmc="平时(50%)"`+`xmcj` | 单打主表只有总成绩，没有平时/期中/期末分项；要做 N+1 查询或前端合并展示 |
| **N551225 修业情况**（§2.6）| **1+N pattern**：`xsxyqk_ckXsXyxxHtmlView` overview + `xsxyqk_ckDynamicGridData` × 20 详表，`xfyqjd_id` 串联；overview items 含 `level2/level3/level4` HTML 串和 `zgshzt`(Y/N) | 1 次拿不全；overview 里 level2/3/4 是 ZF 拼好的展示 HTML，不是结构化数据 |
| **N551225 修业情况**（§2.6）| **`xh_id` 在 URL，不在 form**——独此一家 | form 里塞 xh_id 会被 ZF 忽略，端点用当前 session 默认值；URL 里漏了会 4xx |
| **N153521 培养计划**（§2.8）| **默认返 412 行全校所有专业**；CLI 必须 form 带 `zyh_id` + `njdm_id` 过滤 | 不过滤直接落库会扫全校；item 里有 `xsdm_0X`（X 为动态数字），字段名按学年学期变 |
| **N532560 毕业设计**（§2.9）| 当前用户非毕设阶段时 items 空；页面顶部 "当前毕业设计学年学期:2018-2019" 是**stale display** | 误判端点挂；CLI 区分"空 items + 200" 与"4xx" 两态 |
| **N2154 周课表**（§2.7）| `oldzc` = **16-bit 周次位掩码**（bit i = 第 i+1 周有课），`oldjc` = 节次位掩码；`rqazcList[]` 给 weekday→真实日期 map | 解析 `zcd` "1-16周"/`jc` "3-4节" 字符串既不准也累，bitmask 一行 `(oldzc >> (week-1)) & 1` 搞定 |
| **N2151 / N2154 学期编码**（§2.2 §2.7）| `xqm` 编码：**3=第1学期 / 12=第2学期 / 16=第3学期**（反直觉） | 当成 1/2/3 传 ZF 会返空 items 不报错 |

**通用形态约束（重申，所有 SP 共享）**：
- ZF 序列化全是 String —— `xf` / `jd` / `totalCount` 全部 String，CLI 自己 deserialize
- `cj` 字段是 String 但内容混合："P" / "W" / 字母等级 / 数字字符串 —— **永远不要 force parse to f64**
- 标准分页 envelope `{currentPage, pageNo, pageSize, totalCount, totalPage, totalResult, items}` 抽 `JwcPage<T>` 一次写；非分页接口（GPA/overview）用 `Vec<Value>` 或专属 struct
- 两阶段端点的"触发 phase" response 经常是裸字符串（`"统计成功！"`、`"true"`），用 `Value` 兜底，别用 struct

**错误模式**（实装时最容易犯的）：
1. 把 N305005 的 form-only POST 范式套到 N309131（漏一阶段）/ N551225（漏 xh_id-in-URL）/ N358105（漏 ?su= query）
2. N153521 不带过滤上线，第一次调用就把 412 行全校数据回到日志/缓存里（隐私事故）
3. 课表展示从 `zcd`/`jc` 字符串解析周次节次（脆 + 慢），忘记 `oldzc/oldjc` 位掩码现成
4. `xqm` 用直觉值 1/2/3，调试半天看不出为什么 items 空
5. `cj` 当 f64 反序列化，遇到 "P"/字母直接 panic / 默认 0.0

**正确做法**：
1. 实装每个 SP 前先看 `isjtu_investigation.md` §2.x 的 form / URL / response 例子，**严格按调研期抓的形态**写，不要外推
2. CLI 抽 `JwcPage<T>` 只服务"标准分页"那批；GPA / overview 这种异形端点写专属 struct，不要硬塞分页 envelope
3. `oldzc/oldjc` 位掩码解析写一个 util，所有课表 SP 共用
4. `xqm` 编码写常量 `XQM_AUTUMN=3 / XQM_SPRING=12 / XQM_SUMMER=16` + doc comment 解释为啥不是 1/2/3
5. `cj` 字段 type = `String`，展示层再决定是否尝试 parse；模型层 `Cj(String)` 包一层防 force-parse
6. N153521 端点 CLI 强制要求 `--major <zyh_id>`（或从 session 推断），无 zyh_id 不让跑

**规则**（按"实装时一行 grep"标准写）：
- ✅ ZF 异形端点表见此 lesson 表格；新 SP 实装前先对照
- ✅ `JwcPage<T>` 只用于"items 数组在分页 envelope 里"那批；异形端点别套
- ✅ ZF String-only 序列化 → 模型层全部 `String`，业务层再 typed
- ✅ `cj` / `bfzcj` / `xf` / `jd` 模型字段一律 `String`；不在 deserialize 期试图 parse
- ✅ `xqm` 用常量，**永不**直接传 1/2/3
- ✅ `oldzc/oldjc` 解析走位掩码 util，**永不**parse `zcd`/`jc` 字符串
- ❌ 不要把任意 ZF SP 假设为"单 POST 拿 items"——5/9 的 SP 都不是
- ❌ 不要把 N305005 的 form 字段名集合照搬到其他 SP；每个 SP 字段不同（N551225 是 xh_id-in-URL，N358105 是 ?su=，N309131 是两阶段）
- ❌ 不要在 N153521 实装上线前漏掉 zyh_id 过滤——一次误用 = 412 行全校落日志

**当前代码状态**：
- ✅ 9 SP 规格全归档 `tasks/isjtu_investigation.md`
- ⏳ `apps::jwc/` 未起；起手第一件事是 `JwcPage<T>` + ZF client + 4 个核心 SP（N305005 / N2151 / N309131 / N358105）handler

---

## 2026-04-26 — 水源 self-delete top-level topic 站点级禁用 + 测试帖 raw 必须伪装

**触发情境**：CP-W4 真机：`sjtu shuiyuan new-topic "[CP-W4] sjtu-cli 自动化测试 请忽略" "本帖由 sjtu-cli new-topic 自动化测试 (CP-W4) 发布..."` → 200 返 `topic_id=469507 / post_id=8805252 / cooked` 三件套。立即 `sjtu shuiyuan delete-topic 469507 --yes` → **422 "删除该话题时出错。请与网站管理员联系。"**；改 `delete-post 8805252` → **403 "您没有权限查看请求的资源。"**（首楼保留）；75s 后重试 delete-topic 仍 422，排除 per-minute 限流。让用户 web 上手工删 → 弹窗"**您无权删除此话题。如果您确实希望将其删除，请提交举报并说明原因，以便引起版主注意**" —— 是水源 site-wide 配置硬约束，与 trust level / per-day 配额无关。同时观察到水源对未带 `--category` 的 topic 自动重分类到"水源广场 谈笑风生"，并由 `shuiyuan-bot` 用户自动跟一帖："请勿选择未分类，也请不要随意发在聊聊水源..."。最终用户在 web 上手工编辑标题/首楼把 raw 改成中性"加油喵～/加油做最好的自己"无害化收尾，CLI 没有 edit-post 端点没法自动做。

**错误模式**：
1. 假设 04-24 reply→delete-post 路径成功 = delete-topic 在 self-created top-level topic 上也行得通（实际两条路径权限不同：reply 创建的 post 用户可删，self-create 的 topic 用户级不可删）
2. 把 422 第一反应解读为 per-minute 限流（搜 Discourse meta 看到 max_post_deletions_per_minute 设置就跑偏），75s 后重试才证伪
3. **测试帖内容直接把 `sjtu-cli` / `CP-W4` / `自动化测试` 字样写进 raw**，cooked 渲染后是裸奔的 HTML，所有水源用户都能看到 bot fingerprint，删不掉时事故面积扩大
4. 不传 `--category` 直接发，没意识到水源会自动归到 uncategorized + 触发 shuiyuan-bot 警告 + 进首页 latest 流

**正确做法**：
1. 水源任何 destructive 写操作 CP 之前先想"如果删不掉怎么办"——预设 fallback：edit raw 中性化、举报让 mod 删、或干脆不发
2. 测试帖的 raw / title 必须像正常用户随手发的话题（"加油"、"测试一下输入法"、"今天天气真好"），**永远不在内容里写 CLI 名 / 任务编号 / `自动化` / `bot` / `测试请忽略` 字样**——出事时事故面积小一个数量级
3. 422 + "请与网站管理员联系" 不是限流；**Discourse 错误文案"contact site administrator"通常 = site setting 级 enforcement**，不是 trust level 也不是配额，重试无意义
4. new-topic CP 默认带 `--category`（先查一个允许 self-delete 的版块 id；或者别 CP delete 路径，只 CP post 路径）

**规则**：
- ✅ 水源 site setting 对普通用户禁用 `DELETE /t/<id>.json` 删 self-create 的 top-level topic；唯一删除路径 = flag→mod。CLI 拿 422 是 server 在执行规则，不是 bug
- ✅ 水源 reply→delete-post 路径仍可用（删自己回复别人帖产生的 post），但 self-create new-topic→delete-topic 路径不可用；**两条路径权限模型不同，不要互相外推**
- ✅ 水源未分类 topic 会被 site auto: 自动重分类 + `shuiyuan-bot` 跟帖警告 —— 想低调测试就别走默认 category
- ✅ 任何水源写测试，raw / title 必须是日常水源用户口吻（无 CLI 名 / 无任务编号 / 无 "测试" 字样），删不掉时也无害
- ❌ 422 "请与网站管理员联系" 不是 per-minute / per-day 配额，不要盲目重试 —— 直接看 site setting / mod 路径
- ❌ 不要把 04-24 的 delete-post 真机验证经验外推到 delete-topic，两端点是不同的权限
- ❌ 不要假设"反正有 delete-topic 兜底"就发暴露字样的测试帖

**当前代码状态（2026-04-26 CP-W4 收尾）**：
- ✅ CP-W4 上行：`new-topic` 不传 `--category` 时落 uncategorized → 水源自动重分到"水源广场 谈笑风生"，post 200 返 PostCreated 三件套，写路径 verified
- ❌ CP-W4 下行：`delete-topic` 在 self-create top-level topic 上 422（site-wide enforcement，非 CLI bug）；`delete-post` 在首楼 403（首楼保留），唯一收尾路径走 web 编辑或 flag→mod
- 📌 469507 通过 web UI 手工 edit 标题/首楼无害化（标题"加油喵～"/ 首楼"加油做最好的自己"），bot fingerprint 消除
- 📌 后续若有自动化 edit 需求可加 `PUT /posts/<id>.json` 端点（性价比低，目前不做）

---

## 2026-04-26 — 水源 PM 字段名 + 删除语义都魔改

**触发情境**：CP-PM1 真机跑 `sjtu shuiyuan pm-send 百合师傅 ... --yes` → 422 "您必须选择一个有效的用户。"。第一反应是 username 不对：试 `vladimirr`（current_user.name）也 422。试 `target_recipients=百合师傅` （用 form-urlencoded、共享 cookie jar、fresh CSRF）→ **200 创建成功**，PM id=8804344。继续：发出去的 PM 不在 inbox（自发不进自己 inbox），在 sent 里显示。`sjtu shuiyuan delete-topic 469487` 返 `deleted: true` 但 GET /t/469487.json 仍 200 完整内容 + 头有 `X-Discourse-Route: topics/destroy` —— DELETE 接口 server 返 200 但**对 PM 不实际生效**。最终用 `PUT /t/<id>/archive-message.json` 才让 PM 从 sent 视图消失。

**错误模式**：
1. 假设水源 Discourse 完全沿用标准 `target_usernames`字段名，没去 grep 水源前端实际请求或试备选名。
2. 看到 `delete-topic` 返 200 + `deleted: true` 就认定真删了，没对 GET /t/<id>.json 做交叉验证。
3. CLI 的 `finish_empty()` 只看 status 2xx，不读 body 不验落地状态——给"DELETE PM 成功"假象。

**正确做法**：
1. 写水源端点先用 form-urlencoded + 真 cookie jar 试 `target_usernames` / `target_recipients` 两组——错误信息差异最快定位字段名（"必须选择有效用户" = 字段不被识别 / "未找到该用户" = 字段对值不对）。
2. 写完 PM 测试自删时 **GET /t/<id>.json 二次验证 deleted_at 字段非空**，仅看 DELETE status 不够。
3. PM 类 topic 想清理走 `PUT /t/<id>/archive-message.json`（archive，软"归档"，从 sent/inbox 移走但仍可在 archive 视图找回），不要走 `DELETE /t/<id>.json`（对 PM 是 no-op）。

**规则**：
- ✅ 水源 PM 写端点字段名 = `target_recipients`（不是标准 Discourse 的 `target_usernames`）
- ✅ 水源 PM 删除语义 = `archive-message`（PUT），不是 `destroy`（DELETE）。`DELETE /t/<id>.json` 对 PM 静默 no-op
- ✅ 水源任何"自定义 fork 字段名"嫌疑场景：用 `target_*=alice` / `target_*=百合师傅` 真账号最小 curl 跑两组，error message 就告诉你哪个对
- ✅ 写端点 CP 必须双向验证：写完 GET 一次确认落地（不只看写接口的 status 码）
- ❌ 不要假设水源 == 标准 Discourse 的 API 形状，水源是 fork 已经多次魔改（field name / cookie / route）
- ❌ 不要拿 `finish_empty()` 给 PM destroy 这种"server 返 200 但实际无效"的端点背书

**当前代码状态（2026-04-26 当晚补丁）**：
- ✅ `apps::shuiyuan::api_write::archive_pm` 已上：PUT `/t/<id>/archive-message.json` + CSRF + `finish_empty`
- ✅ `commands::shuiyuan::cmd_delete_topic` confirm 通过后先 `client.topic(id, 1)` 取 `archetype`，是 `private_message` 时 `anyhow::bail!` 指向 archive-pm，PM 路径不再 silent 假成功
- ✅ `models::TopicDetail` 加 `archetype: Option<String>` 字段以支持上述预检
- ✅ CLI 新命令：`sjtu shuiyuan archive-pm <topic_id> [--yes]`
- ✅ 真机 CP-PM2 + CP-DT-PM 双绿（topic 469498 走 archive-pm 让 sent returned 1→0；topic 469500 跑 delete-topic → 友好错指向 archive-pm，不进 silent no-op）

---

## 2026-04-25 — release binary 过时，调试前先核 freshness

**触发情境**：跑 `sjtu shuiyuan login-probe` 报 `error sending request`，连续 30+ 分钟在网络层（HTTPS_PROXY env / TLS / Clash 端口）打转。先怀疑 reqwest 默认代理行为，又写 `examples/proxy_diag.rs` 三组 builder 对照，全部白干。最终 `stat target/release/sjtu.exe` + `find src -name "*.rs" -newer target/release/sjtu.exe` 才看出 binary 是 2026-04-23 16:55 编的旧版，比 `apps/shuiyuan/http.rs` 当前源码（含 `pool_idle_timeout(0)` 修复）旧 2 天 —— `cargo build --release --bin sjtu` 重编后立刻通，CP-1..6 + CP-M1/M2 8/8 一气过完。

**错误模式**：把"运行行为异常"直接等同"代码 / 网络栈有问题"，跳过"binary 是否对应当前代码"这一步直接深挖；多次重跑得到一致错误就更确信"代码有问题"，没去验 binary 时间戳。

**正确做法**：sjtu CLI 跑时行为和源码 / 注释明显不一致 → 第一步：
- `stat target/release/sjtu.exe` 看 mtime
- `find src -name "*.rs" -newer target/release/sjtu.exe` 看是否有更新源
- 任一命中 → 立即 `cargo build --release --bin <name>` 重编再继续诊断

**规则**：调试 sjtu CLI（或任何 cargo release binary）运行时异常 / 行为不符合源码描述：
- ✅ Step 0 = `find src -newer <binary>` 验 binary 是否过时
- ✅ 任何"注释里写了 X、行为表现不像 X"的情况，第一假设永远是 binary 旧
- ✅ rebuild 比写 minimal repro / 加 RUST_LOG=trace 都便宜得多
- ❌ 不要直接跳到 reqwest/hyper trace 日志或新建 examples 复现
- ❌ 不要假设"binary 还是上次编的那份" —— 中间有 edit / commit / git pull，就可能旧

---

## 2026-04-22 — 有明确参考时不扩展调研

**触发情境**：用户让我规划 SJTU-CLI 并已指明"仿照 xiaohongshu-cli 的 QR 扫码登录方式"。

**错误模式**：我仍然并行发起 4 个 WebFetch，去研究 `developer.sjtu.edu.cn` 的 OAuth 开发者文档、OIDC 流程、开发者平台能力等"替代方案"，被用户中断。

**正确做法**：用户已经明确参考时，直接读参考项目的实现、按参考实现做适配即可，不要再扩展调研其他方案。

**规则**：触发词 = "仿照 / 参照 / 按 X 方式 / 跟 X 一样 / 复刻 X"。触发时：
- ✅ 读参考项目的源码
- ✅ 对照参考项目做本项目适配
- ❌ 不再 WebFetch / WebSearch 研究替代方案
- ❌ 不再"为了完备性"补充上下文
- 有不得不澄清的歧义：用 AskUserQuestion 问用户，不要自己 fetch

---

## 2026-04-23 — mockito + reqwest 测试必须 `.no_proxy()`

**触发情境**：S3a 写完水源 OAuth2 链后跑 `cargo test`，auth/cas 和 auth/oauth2 两套 mockito 跟链测试同时 6 个挂：`Expected 1 request(s)... but received 0`、部分返 503、redirect-loop 测试本应报错却返 Ok。

**错误模式**：以为 `reqwest::Client::builder()` 什么都不配就是"干净 client"。实际它默认走 `Proxy::system()`，会读本机 `HTTP_PROXY` / `HTTPS_PROXY` 环境变量。本机装了 Clash/V2ray 代理（`http://127.0.0.1:10808`），于是：
- mockito 起在 `127.0.0.1:random_port`
- reqwest 把请求先发给 `127.0.0.1:10808` 代理
- 代理把请求当成"要走上游"，要么超时、要么错路由、要么返 503
- mockito 永远收不到请求，`expect(1)` 断言挂

**正确做法**：`Client::builder()` 链上加 `.no_proxy()` 强制不读环境变量。只针对单测的 `bare_client()` 加，生产 client 不改（生产走代理是合法需求）。

**规则**：任何 `mockito::Server` + `reqwest::Client` 的测试：
- ✅ 测试用 `Client::builder().no_proxy()`
- ✅ 短 timeout（5 秒够了）防止代理劫持后长挂
- ✅ 注释里写明"为什么加 no_proxy"，提醒后来人别去掉
- ❌ 不要依赖 CI 环境无代理—本地开发机多半装了代理
- ❌ 不要为此去改 HTTP_PROXY 环境变量（副作用太大）

---

## 2026-04-22 — headless_chrome 抓 cookie 必须跨域

**触发情境**：S1 扫码登录链路里，用户扫码完跳到 `my.sjtu.edu.cn/ui/app/`，我用 `tab.get_cookies()` 想抓 `JAAuthCookie`，结果空。

**错误模式**：以为 `tab.get_cookies()` 返回浏览器里所有 cookie。实际它底层调 CDP `Network.getCookies`，**只返回当前 tab URL 关联的 cookie**。`JAAuthCookie` 设在 `jaccount.sjtu.edu.cn` 域，从 `my.sjtu.edu.cn` 抓不到。

**正确做法**：跨域抓 cookie 用 `tab.call_method(headless_chrome::protocol::cdp::Network::GetAllCookies(None))`，返回 `Vec<Cookie>` 含所有域。

**规则**：headless_chrome 里抓 cookie，**默认就用 GetAllCookies**，除非确定只想要当前 URL 那个域；任何 SJTU 多子域跳转流程更不能用 `tab.get_cookies()`。

---

## 2026-04-22 — JAccount bare URL 是欢迎页不是登录页

**触发情境**：S1 想让 Chrome 打开 JAccount 登录页扫码，把入口写成 `https://jaccount.sjtu.edu.cn/jaccount/`，结果只看到一行 "Welcome to SJTU jAccount"，没有 QR。

**错误模式**：以为 JAccount 域名根目录就是登录入口。实际它是 SSO 中心，登录页要由 SP（service provider）通过 CAS 重定向参数（`?sid=...&service=...&...`）触发出来。

**正确做法**：入口直接用 SP 的 URL（如 `https://my.sjtu.edu.cn/ui/app/`），未登录时 CAS 自动跳到带 QR 的真正登录页；扫码完又跳回 SP，刚好是成功标志。

**规则**：触发任何 SJTU SSO 子系统的登录流程，**永远从 SP 的目标 URL 进**，不要直接访问 jaccount 域。S2 CAS 跳转复用同一逻辑：`navigate_to(target_sp_url)` → `wait_until_navigated` → 看 URL 决定是已登录还是要走 CAS。

---

## 2026-04-22 — reqwest 自动 follow redirect 会吞掉中间 Set-Cookie

**触发情境**：S2 做 CAS 通用通道，想让 `reqwest::Client` 打目标 SP → 自动跟 jaccount → 自动跳回 SP，然后把最终 cookie 落盘给各子系统复用。

**错误模式**：第一反应用 `reqwest::Client::builder().redirect(Policy::limited(10))`（默认就是它）+ `cookie_store(true)`，以为 cookie store 会把链路上所有 `Set-Cookie` 都收进来。实际：reqwest 自动跟 redirect 时**会把中间响应吞掉**（response body/headers 都对我们不可见），`resp.cookies()` 只能看到**最后一跳**的 `Set-Cookie`。中间 jaccount 设的 session cookie、SP 第一跳设的 JSESSIONID 都拿不到。且 `reqwest::cookie::Jar` 没有公开的"列出所有 cookie"方法。

**正确做法**：手动跟链 —— `Policy::none()` 禁自动 redirect；循环 `client.get(url).send().await`，每跳用 `resp.cookies()` 累加到 `HashMap<(name, domain), Cookie>`，再按 `Location` 头 `url.join(loc)` 算下一跳 URL。循环上限给 10 防死循环。`cookie_store(true)` 仍然开着——jar 负责"下次请求带 cookie"，我们自己负责"全链路记账"，两套不冲突。

**规则**：reqwest 做 CAS / OIDC / 任何多跳 SSO 链时：
- ✅ `redirect(Policy::none())` + 手动 `for ... client.get(url).send()` + 每跳收 `resp.cookies()`
- ✅ 用 `(name, domain)` 复合键去重，别只用 name（同名不同域 cookie 会被覆盖）
- ✅ 每跳后 `is_redirect(status)`；非 3xx = 终点
- ✅ 终点验落点域：停在 IdP 域 = IdP cookie 失效 or 该 SP 需要交互确认 → 主动报错别默默返回空 session
- ❌ 不要依赖默认 `Policy::limited(N)` + `cookie_store(true)` 的组合来"自动收齐 cookie"
- ❌ 不要指望 `reqwest::cookie::Jar` 暴露 `list_all()` 方法（没有）

---

## 2026-04-22 — Cookie 唯一键必须是 (name, domain, path) 三元组

**触发情境**：S2 收尾后想给 `Session::redacted()` 加一个"同名不同域复合键"去重，用户说"联网交叉验证无误后严格准确地执行"；WebFetch 查 RFC 6265 才发现我准备的 `(name, domain)` 二元组依然不够严格。

**错误模式**：想当然以为 "name + domain" 就能唯一标识 cookie。S2 的 `follow_redirect_chain` 和 `redacted()` 都是这套思路。

**正确做法**：RFC 6265 §5.3 明确 cookie 唯一键是 **(name, domain, path) 三元组**——同名同域但不同 path 是两条独立 cookie。`cookies::Cookie` struct 要有 `path: Option<String>`；所有跨 cookie 的集合去重都要用三元组；脱敏 key 格式 `name@domain,path`。reqwest `Cookie::path() -> Option<&str>`、headless_chrome CDP `path: String`、rookie `path: String` 都能填出这个字段。

**规则**：任何 cookie 集合（HashMap / HashSet / BTreeMap）的 key：
- ✅ `(name, domain, path)` 三元组，缺省值保留 `""` 参与区分
- ✅ 序列化/展示时 `name@domain,path`，空用 `-`
- ❌ 不用 `name` 或 `(name, domain)` —— 后者只修了 50%
- ❌ 不省 path 字段。即使当前子系统只出现一条同名 cookie，改版时翻车难追

另：触发"严格"+"正确性"关键字时，**联网交叉验证是一级工序，不是可选项**。这次不是验证出来就是按错的实现落盘了。

---

## 2026-05-10 — V5.A LTI Bootstrap 缓存：行数估算与 PowerShell 计行数的双重坑

**触发情境**：CP-V5.A 实装跑 subagent-driven flow，14 task 拆细 + 每 task 5 步（write test → fail → impl → pass → commit），plan 估 cache.rs ~100 行 / handlers.rs +25 行，实装时两次撞 200 行硬限。

**错误模式 1**：plan 阶段对单文件代码量估偏小。`save_to_path` + `chmod_600` cfg-gate + `clear` 三路分流（具体文件 / 按 prefix 扫 / 全清）拼起来 cache.rs 实际 200 行（plan 写 ~100 行）。`with_token_refresh` 加 doc + where clause + 新 imports 让 handlers.rs 从 166 直撞 200（plan 写 +25 = 191）。

**错误模式 2**：PowerShell `(Get-Content $f | Measure-Object -Line).Lines` 在 Windows CRLF 文件上**漏报 ~12 行**。subagent 报 download_handler.rs "194 行" → `wc -l`（git bash）实测 215 行。差异源于 `Measure-Object -Line` 数 newline char 的逻辑跟 Windows CRLF 处理有边界 case。

**正确做法**：
- plan 阶段对 cache / retry / shared helper 类文件按"骨架 + impl + cfg-gate + clear 类逻辑"4 块独立估行，每块 30-50 行起步，留 30% 余量
- 行数验证统一走 `wc -l`（git bash）或 `(Get-Content $f).Count`（PS array length），不用 `Measure-Object -Line`
- 单文件接近 200 行时立刻评估拆分，不等 200 后被动救火（V5.A 撞 200 → 临时 T8a 拆 retry.rs / T13a 拆 download_shared.rs，破坏了 commit 历史的整洁性，commit 数从 plan 的 14 个膨胀到实际 16 个）

**规则**：
- ✅ plan 阶段对每个新建 .rs 估 1.3-1.5× 实装代码量留余量
- ✅ 行数大盘统一 `wc -l`，不用 PowerShell `Measure-Object -Line`
- ✅ 单文件 ≥ 180 行立刻评估拆分（不等 200 撞墙）
- ❌ 不靠 subagent 自报行数（CRLF 计数差异 + 报告精度问题）；自己 `wc -l` 兜底

---

## 2026-05-10 — sub_session 本地 TTL 不等于服务端 cookie 有效期

**触发情境**：V5.A 真机 4 关跑关 1 时，`Client::connect → cache 未命中 → auth::lti_launch → cas_login("canvas_oc")` 路径走完，cas_login 看 `canvas_oc.json` 软 TTL 在 30 天内（2026-05-08 落盘 → 2026-06-07 软过期），跳过 CAS 重 handshake；Chrome 用陈旧 cookies 访问 oc.sjtu LTI URL → oc.sjtu 服务端 session 已实际失效，redirect 回 `/login/canvas` 静态页 → 30s 超时报"LTI 落地超时"。

**错误模式**：`cookies::Session::is_expired` 用 `captured_at + 30 天` 软 TTL 标记，服务端 cookie 真实失效时间通常远短于此（oc.sjtu 实测 ≤ 2 天）。本地 TTL 通过不等于服务端会接受。

**正确做法**（V5.A 没修，因属预存边界 + 跟 V5.A 缓存正交，但记下来供后续 phase）：
- sub_session 文件应记**服务端真实 expires**（看 cookie 自身的 `expires` 字段，不是 captured_at + 固定值）
- cas_login 路径上发现 oc.sjtu 重定向回 `/login` 类页面时：自动清 sub_session + 重 CAS 一次，不要一路走到 Chrome 30s 超时
- 用户侧绕路：删 `~/.sjtu-cli/sub_sessions/canvas_oc.json` → cas_login 走完整 OIDC redirect chain（不需要重扫码，主 session.json 仍能签）

**规则**：
- ✅ 服务端 session 失效 ≠ 本地文件 TTL 失效；做缓存层时两个时钟分开看
- ✅ Chrome / reqwest 拿到陈旧 cookies 被踢登录页时，要**主动**清缓存重 auth，不要让超时机制兜底
- ❌ 不要把"文件落盘 mtime + 30 天" 当作 cookie 服务端有效期的代理
- ❌ 不要在子系统全链路 30s 超时后才意识到 sub_session 服务端死了

---

<!-- 新的经验追加到此处上方，最新在上 -->
---
## 2026-05-11 — V5.D mp4 真实布局 + sample-level Range 工程妥协（audio-only 直下 m4a）

**触发情境**：V5.B baseline 前 9 讲实测 sustained 20.7 min/讲、下 840 MB mp4 只为抽 20 MB m4a（浪费 42×）。V5.D 设计目标：parse mp4 moov → Range-fetch audio sample → 本地 mux m4a，跳过 mp4 落盘 + 跳过 ffmpeg。Phase 1 真机 smoke 一讲（L10）跑通：exit 0 / download_kind=m4a-direct / m4a 22 MB。Phase 2 9 讲 batch 跑前先把工程妥协写下来防 V5.E 重蹈。

**错误模式**：
1. **moov 位置假设错** —— V5.D 初版 `locate_moov` 用"头 1MB 探测 + 尾部翻倍 16MB 探测"策略，但 SJTU CDN mp4 是 `[ftyp 24][mdat 914 MB][moov 2.1 MB]` 三 box 结构（standard layout，mdat 占 99.7%），尾部翻倍探测从 `total - probe` 倒退总落在 **mdat 内部**，把 mdat 随机字节当 box header 解析 → 永远找不到 moov，head 探测失败后整个流程 bail
2. **sample-level Range 含大量 video noise** —— 初设 `gap_threshold=64 KB` 让相邻 audio sample 合并。实测 mp4 是 **per-sample chunk** 布局（每 audio sample 独占一个 stco entry），audio sample 之间在 mdat 内被 video frame 分隔。gap=0 → 55699 Range CDN 不友好（HTTP overhead 主导）；gap=64KB → 1201 Range 但单段含大量 video（705 MB 下载得到 22 MB m4a，浪费比仍 32×）
3. **新路径无降级直切** —— 第一版 V5.D 失败时直接 bail，用户拿不到任何产物。批量下载里这是灾难（前 8 讲跑完第 9 讲 bail 等于浪费 2h）

**正确做法**（已落 V5.D 实装）：
1. **从合法 box 边界开始 scan**：head 探测时追踪 mdat box header 的 size 字段，推算下一个 box 起点（`ftyp_end + mdat_size`），从那里 fetch moov。绝不假设任意 offset 是 box 头
2. **诊断 hook 内嵌生产代码**：`dump_top_boxes(buf, max_n)` + `hex_prefix(buf, n)` 写进 `locate.rs`，`RUST_LOG=info` 让 CDN 真实 box 布局直接出现在 stderr。CDN 行为是黑盒，临时 println! 改了再删每次都得重写
3. **保留旧路径 + 自动降级**：`download_shared.rs` 用 `match Ok / Err → tracing::warn + 落到旧 mp4-full 路径`，V5.D 失败时用户依然拿到 m4a，仅 elapsed 长（21 min 而非 ~3 min）但 envelope 完整
4. **调研期 env 切换 fail-soft**：加 `SJTU_NO_FALLBACK=1` env var：V5.D 失败时 bypass fail-soft 直接 bail。调研期用它快速验证 V5.D 是否真工作（不被 21 min mp4-full fallback 浪费时间）。默认行为仍是 fail-soft 不变

**真机验证**：
- L10 single-lecture smoke：V5.D 主路径成功（download_kind=m4a-direct），mp4 layout 在 stderr 完整暴露（ftyp 24 / mdat 914 MB / moov 2.1 MB）
- 1201 Range 合并 vs 55699 sample 直下：合并后 elapsed 显著降，但下载字节 705 MB（非理想 22 MB），是 V5.E chunk-level Range 的动机
- fail-soft 兼容：去掉 `SJTU_NO_FALLBACK` 时 V5.D 失败自动落到 mp4-full + ffmpeg，envelope 仍 status=ok

**规则**：
- ✅ **mp4 box scan 必须从合法 box 边界开始**，不能假设任意 offset 是 box 头。Why：mp4 是 box 流，box 之间无 magic separator，从中间字节解析会把 payload 当 header 长度。How to apply：head 探测时记录每个 box `[type, size]`，要 jump 用累加 size，不用 `total - probe` 类倒推
- ✅ **新优化路径必须保留旧路径 + 自动降级**：不要硬切。Why：批量下载场景下游 SP 行为不可控，新路径首次真机跑挂概率非零；硬切等于把"探索"成本转嫁到用户已投入时间。How to apply：用 `match Ok/Err` 在外层包，Err 路径走旧实现并 warn 一条 stderr，envelope 加 `download_kind` 字段标当前用了哪条路径
- ✅ **调研用 debug switch 走环境变量，不污染默认行为**：Why：默认 fail-soft 对用户友好，但调研期 fail-soft 会掩盖新路径真实状态；CLI flag 暴露在 help 文档里是承诺向后兼容的负担。How to apply：调研开关用 `SJTU_NO_FALLBACK=1` / `SJTU_DEBUG_FOO=1` 之类 env var，文档只在 `tasks/lessons.md` 提，不进 `--help`
- ✅ **诊断 hook 内嵌生产代码 + RUST_LOG 控制**：Why：CDN / 第三方协议黑盒，临时 println! 改了再删每次重抓信息成本高；写进生产代码用 tracing level 控制开关零成本。How to apply：解析关键数据时（mp4 box scan / TLS handshake / CAS redirect）写 `tracing::debug!(...)` 把核心字段打出来，bug 发生时 `RUST_LOG=debug` 一开即见
- ⚠ **sample-level Range 合并对 per-sample chunk 布局 mp4 仍含大量噪声**：V5.D 当前实现的工程妥协，gap_threshold=64KB 是 1201 Range 与 705 MB 下载量之间的平衡点。Why：audio sample 在 mdat 内被 video sample 分隔，gap=0 会爆 HTTP overhead。How to apply：V5.E 改 chunk-level Range（stco/stsc 表 chunk 整体下），可把 705 MB → ~22 MB 精确等于 m4a 大小

---

## 2026-05-12 — V5.F 撤回 audio-only 整路：3 轮优化失败 → 回归 V5.A baseline

**触发情境**：V5.B/D/E-B+ 三轮 audio-only 优化（每轮 4-8 day 实装 + 真机）全失败。V5.B mp4 chunk-level Range：CDN per-sample chunk 布局让 audio chunk 之间被 video 隔开，"audio chunk 整体下"实际下 705 MB；V5.D sample-level Range：1201 Range 仍 705 MB 浪费 32×；V5.E-B+ 4-Client H2 池 + Dynamic P85：基于"H2 multiplexing 1.5× 提速 + P85 自适应 gap"两个论文级假设，真机反向退化（21.6 min/讲 vs V5.A baseline 18 min/9 讲）。最终 V5.F 删除 audio_dl/m4a_mux/mp4_box 3 个目录 1500 行 + 撤回到 V5.A mp4-full + ffmpeg 单路径，9 讲 batch 15.13 min ≤ 25 min 目标。

**错误模式**：
1. **CDN 真实约束没用一次性脚本测，直接按论文/AWS S3 文档/H2 RFC 推断写代码** —— SJTU CDN 实际是 NGINX 单连接 sustained throughput cap 11.4 MB/s（这是 fat-pipe 限速，不是 RTT 瓶颈），H2 多路复用对单大文件 throughput-bound 任务**等价或更糟**，因为 multiplexing 把单连接带宽切给多个 stream 反而降低有效吞吐
2. **B.1 ffmpeg stdin pipe 优化假设 mp4 是 faststart 格式** —— 实际 SJTU CDN mp4 是 `[ftyp 24][mdat 914 MB][moov 2.1 MB]` 三 box 的 moov-end 布局，ffmpeg stdin 无法 seek 回头读 moov，5 min hexdump 一眼看出，省下 1-2 day 写完后才发现的代价
3. **新优化路径 fail-soft 自动降级掩盖真实退化** —— V5.E-B+ 用 `match Ok/Err → warn + 落到 mp4-full` 兜底，9 讲 batch envelope 里全部 status=ok，但实际每讲都从 H2 池失败降级回 mp4-full，21.6 min/讲完全被 fail-soft 字面成功包住，跑完后看 elapsed 才发现退化
4. **优化路线把主线 stake 在未验证假设上** —— V5.B/D/E-B+ 三轮都在主分支推进，每轮都假定上一轮"接下来 V5.X 会修好"，导致 task #39/#41/#42 都是"延后到下一轮"状态，最终 V5.F 收尾时累计撤回 3 个目录 1500 行 + 2092 deletions

**正确做法**（已落 V5.F 决策）：
1. **CDN 性能优化前先用一次性脚本测真实约束**：curl 测单连接 throughput / hexdump 验 mp4 box 布局 / RUST_LOG=trace 看 reqwest H2 帧。5-30 min 验证 + 1-2 day 实装的成本比是 1:60；用 30 min 验证否决一个错误假设的省时是几天计
2. **fat-pipe 限速 vs RTT 瓶颈先分清再选 H2/H3**：实测 11.4 MB/s 持续单 H1 流跑满 → throughput-bound；只有 RTT > 100ms 且单文件 < 10 MB（多请求并发场景）H2 multiplexing 才有意义。SJTU 校园网内 RTT 5-20 ms，throughput-bound 单大文件**永远不要**上 H2 池
3. **实验性优化路径上线初期关 fail-soft**：用 `SJTU_NO_FALLBACK=1` 之类 env var 跑实验路径，看真实成功率；待验证稳定再开 fail-soft 进生产。fail-soft 是 v1.0+ 用户友好特性，不是 dev/exploration 期工具
4. **优化探索走并行 branch，主线保持可发布 baseline**：V5.A 18 min/9 讲 + 920 MB/讲已经满足 ≤ 25 min 目标，audio-only 加速是"nice to have"。错的是 V5.B 起在主分支推进而非 sidetrack；正确做法是主线 V5.A freeze 同时开 `feat/v5b-audio-only` worktree 探索，每轮真机不达标就丢弃 branch 不污染主线
5. **撤回决策的判定标准用绝对值而非相对值**：V5.E-B+ 反向退化 → 用户问"接下来怎么办"，给的不是"V5.F 继续优化 audio-only"而是"V5.A 已达标，撤所有 audio-only 代码"。判定：当前 baseline 是否已经满足 PRD 目标？若是，所有 nice-to-have 优化达不到就撤；不要因为已经投入 3 轮就继续投第 4 轮（sunk cost fallacy）

**真机验证**（V5.F final）：
- L10 单讲 smoke：1.74 min（目标 ≤ 2.5 min）/ 916 MB mp4 → 21.22 MB m4a / mp4_kept=false / download_kind=mp4-full
- 9 讲 batch：907984 ms = 15.13 min（目标 ≤ 25 min，余量 40%）/ 9/9 succeeded / 0 failed / 7.86 GB 总下载 / 全部 mp4_kept=false + audio_path 落盘 / 各讲 1.49-2.25 min 全过 2.5 min 阈值
- 代码体量：删除 audio_dl/m4a_mux/mp4_box 3 个目录共 1500 行 + 调整 download_shared.rs/data.rs + mod.rs，2 commits 共 -2092 deletions / +90 additions
- 测试：91 unit tests passed（原 ~124，删除 audio-only 路径单测后 -33），clippy `-D warnings` 零警告，fmt 零 diff

**规则**：
- ✅ **CDN/网络优化动手前先一次性脚本测真实约束**：5-30 min 验证省 1-2 day 实装。Why：CDN 行为黑盒，论文/AWS 文档/H2 RFC 是理想模型，实际产品级 CDN 都有限速/限连接/HTTP 版本协商等隐藏约束。How to apply：任何"X 加速 Y%"假设上线前必须有 `curl/hexdump/wireshark` 一次性脚本验证支撑；脚本结果在 spec doc 里贴 hex/curl 输出
- ✅ **H2/H3 多路复用前先分 throughput-bound vs RTT-bound**：单大文件 throughput-bound 场景 H2 等价或更糟。Why：multiplexing 优势在多 stream 共享 TCP/QUIC 拥塞窗口避免 head-of-line blocking，但单 stream 跑满 fat-pipe 时切多 stream 反而降低有效吞吐。How to apply：`reqwest::Client::builder().http1_only()` 在已知 throughput-bound 路径显式锁 H1.1，不要让协议协商自动选 H2
- ✅ **实验性优化路径初期阶段关 fail-soft**：用 env var 切换。Why：fail-soft 是给生产用户的友好兜底，不是给开发者的探索工具；新路径首次真机几乎必然有 bug，fail-soft 会把"全部失败 + 兜底"包成 status=ok 掩盖真实成功率。How to apply：`SJTU_NO_FALLBACK=1` / `SJTU_FORCE_NEW_PATH=1` env var 在 dev 期 default-on，验证稳定后才考虑生产 default-off
- ✅ **优化探索走并行 branch / worktree，主线保持已达标 baseline**：Why：3 轮串行在主分支推进的代价是每轮都 stake 主线进度在下一轮假设上，撤回时累计成本极大（V5.B+D+E-B+ 合计 3 周）。How to apply：主线达标后 freeze，optimization 用 `git worktree add` 开 sidetrack，每轮真机不达标 → 整个 worktree 丢弃，主线零污染
- ✅ **撤回决策按绝对值（PRD 是否满足）而非相对值（投入了多少）**：sunk cost ignore。Why：已投入 3 轮 ≠ 应该投第 4 轮；当前 baseline 满足目标 + 优化达不到 = 撤回，与已投入工时无关。How to apply：每轮优化结束 review 时强制问"当前主线是否已满足 PRD？若是，本轮优化是否达到新承诺指标？"，两个 yes 才合并；否则丢弃

---

## 2026-05-12 — T1 jwc schedule 衍生命令（today/week/next + --grid）

**触发情境**：plan 14 task 全 green（11 task 子代理实装 + cargo test 128/128 + clippy 零 warning），但 T12 真机第一次跑 `sjtu jwc today` 立刻报 `N2154 周课表 JSON 解析失败: null`。挖出 4 个 plan/spec 没覆盖的真实约束 + 1 个老 bug。

**错误模式**：
1. **plan 草稿声明的字段类型与真机 JSON 不符**：plan 写 `oldzc / oldjc: number` + `xqj: string`，T0 fixture 抓出来 `oldzc/oldjc` 是 string ("65535")、`rqazcList[*].xqj` 是 number。但 plan T6/T9 代码块没回头改，stale 类型在 review 期被人眼漏掉
2. **plan / spec 声明"xnm/xqm 留空 = 当前学期"是过时假设**：ZF 老版本接受空值并 server-side fill，9 版本不再 fill，直接返 JSON `null`，无 redirect，无 status code，纯 application-level 失败。T0 fixture 抓的时候我手动给了 xnm=2025 xqm=12，所以 fixture 看不出这个约束
3. **`sjtu logout` 只清主 session，不清 sub_sessions/<name>.json**：cas_login 第一步是 `load_sub_session("jwc")` + `if !is_expired() → return cached`。Apr 26 旧 jwc.json `soft_expires_at` 30 天后（May 26），logout 后 login 调 jwc 命令时直接命中 stale 缓存，返 ZF 不认的旧 cookie → 所有 SP redirect 到 `/xtgl/login_slogin.html`
4. **subagent-driven plan 跨 task 类型漂移漏检**：plan T_N 改了类型定义（T3 把 xqj `Option<String>` → `Option<u8>`），但 T_M (M>N) 代码块还引用旧类型（T6/T9 plan stale `r.xqj.as_deref()`）。subagent 跑 T_M 时按 plan 直接抄会编译失败，靠主对话 controller 在 dispatch prompt 里挖坑前明确告知 corrections

**正确做法**（已落实）：
1. **N2154 / N2151 内部 fallback 推默认 xnm/xqm**：拆 `src/apps/jwc/api/term.rs` `default_xnm_xqm_by_date(today)` 按今天日历月推（2-7 春/8 夏/9-12 秋/1 秋-上学年）。4 个 unit test 覆盖春/秋/夏/边界月
2. **T0 fixture 抓时同时记录 form payload 字段值**：不仅抓 response JSON，还要抓 `cxXsKb.html` 的 POST request payload 看 xnm/xqm/zs 真实值
3. **plan 类型校正必须 propagate 到所有 task**：plan controller 在 T_N 改类型后必须 grep 后续 task 代码块的所有 `.as_deref()` / `parse::<>` / 字段访问，inline 修正
4. **subagent controller 在 dispatch prompt 里显式标注 plan stale 处**：T6/T9 dispatch 我手动加了 "⚠️ plan 第 X 行 r.xqj.as_deref() 是 stale，改成 r.xqj == Some(1)"，subagent 才能避免照抄

**真机验证**（T12 final，2026-05-12 17:30 SJTU 校园网）：
- `jwc today --yaml`：current_week=11 / today_iso=2026-05-12 / today_weekday=2 / items 显日语口译（1）周二 9-10 节 16:00-17:40 ✓
- `jwc week --yaml`：rqazc_list 完整 5/11-5/17 / items 含日语精读周一 3-4 节等 ✓
- `jwc week --zs 1 --yaml`：rqazc_list 显学期第 1 周 2026-03-02 ~ 03-08 ✓
- `jwc next --within 7 --limit 10 --yaml`：fetched_weeks=[11,12] / 10 条按 datetime_start 升序 ✓
- `jwc next --within 31 --limit 30 --yaml`：12.4s（plan 估 ≤5s 偏乐观，5 周 × ~2s ZF RTT+throttle 真实成本）
- cache `~/AppData/Local/sjtu/sjtu-cli/cache/jwc_week_cache.json`：`{"entries":{"__current__":{"week":11,"fetched_at":"...Z"}}}` ✓
- cached run 7.4s（first 12s，节省 4.6s ≫ 500ms 目标）✓
- `jwc today --grid` / `jwc week --grid`：comfy-table 7 列 × 8 节渲染，课程\n教室\n教师 三行 cell

**规则**：
- ✅ **真机 fixture 抓取时同步记录 form payload 真实字段值**：不止 response。Why：response 看不出 server-side 对空字段的容忍度（空也可能 server fill default），只有 request payload 真实值能反推 client 必须传什么。How to apply：chrome-devtools `list_network_requests` 之后 `get_network_request <id>` 看 request body 完整 form-urlencoded，写进 fixture 旁边 `*.request.txt` 一并提交
- ✅ **plan 类型修订必须 grep 整个 plan 文档校正下游 task**：T_N 改类型 → 立刻 grep `as_deref|parse::|\.x[a-z]+` 在 T_{N+1..end} 范围内出现的所有用法，inline 修正。Why：subagent 抄 plan 代码块时不查阅之前 task 的类型定义。How to apply：plan 修订时跑一遍 `grep -n` confirm 没有 stale type ref 再提交 plan diff
- ✅ **CLI logout 必须同时清主 session + 所有 sub_session**：单 `clear_session()` 不够，CAS 子链缓存会让下次 login 后立刻命中过期不严的旧 cookie。Why：`sub_session.soft_expires_at` 是 30 天保守值，但实际 SP session 30 min idle timeout；logout 表达的是"用户想清干净重来"，不只是清主认证。How to apply：`cmd_logout` 调 `clear_session() + glob clear_sub_session_dir()`（已记 todo.md follow-up）
- ✅ **ZF / 老 SP 系统不要假设"空值 = 默认"**：每个查询 endpoint 显式给 xnm/xqm/page/pageSize 等全部字段，不依赖 server-side fill。Why：ZF 各版本对空值容忍度不同，9 SP 收紧后空值直接返 JSON null 无 status code 区分。How to apply：endpoint 默认值放在 client-side 推断（chrono::Local::now() 按月份推），不依赖 server

---

## 2026-05-13 T2 jwc GPA + 排名双轨 + gpa-by-semester

### N309131 两阶段 SP 客户端循环坑

- **step1 必须先发**：跳过 step1 直接 step2 server 返空 items 而非报错，client 无法识别错误源
- **step1 响应是裸 JSON 字符串**（`"统计成功！"`），不是对象 —— serde_json 反序列化时直接 `from_str::<String>`，**不能** `from_str::<Value>`（拿到的是 Value::String 还要再 `.as_str()`）
- **真机 12 学期循环耗时 ~56s**（plan 估算 7-10s 严重偏低）—— N309131 step1 server-side 统计计算本身要 4-5s/次（不是网络 RTT），600ms client throttle 占比 ~10%；agent 调用前要提前告知用户预期等待时长
- **fail-soft 三个 case**：网络挂 / step1 拒绝 / items 空 都装进 `failed` 数组，exit code 始终 0；真机实测大多落到 "items 空" 这个 case
- **xnm-from 默认 "当年-3"**：4 年制本科覆盖率高；非 4 年学制（研究生/留学生）手给 `--xnm-from` 即可，client 不嗅探毕业信息表

### `rank=nj` 在 SJTU 实例不一定支持

- 真机实测：单学期 `--rank nj`（不带专业的纯年级排名）在 server 端返 ZF v5 HTML 错误页，`cmd_gpa` 报"上游响应解析失败"
- 该用户专业方向只 16 人 = 班 = 专业方向，可能 server 视 nj 与 njzy/bj 重复而拒绝
- **agent 建议**：默认用 `--rank njzy`（年级专业）；`bj` 也稳；`nj` 留作探索性参数
- `cmd_gpa_by_semester` 内的 fail-soft 会兜住这种错（match Err → 装 `failed[]`），不会全局崩

### 排名 server-side 给 "X/Y" 字符串 → 双轨 parsed

- 不破坏现有 `gpapm`/`xjfpm` 原字段，**附加** `gpapm_parsed` / `xjfpm_parsed: Option<RankPair>`（JSON 输出 camelCase `gpapmParsed` / `xjfpmParsed`）
- `RankPair.percentile` 在 `total=0` 或 `rank>total` 时为 `None`（fail-soft 而非 panic）
- 用 `#[serde(default, skip_deserializing)]` 让 server 端漂移加字段时反序列化不破，client 端 `Gpa::fill_parsed()` 一次填到位
- 真机：rank=15, total=16, percentile=93.75 形态在 6 单学期 + 多学期 succeeded 全部 ✓

### N309131 `qs_xnxq` + `zz_xnxq` 同传时是"累计截至"语义

- 真机实测：`qs_xnxq` = `zz_xnxq` = "<YYYY><Q>" → server 返 "截至该学期的累计 GPA"，不是"该学期独自的 GPA"
- 例：相邻两学期 `ms` 字段（门数）累加翻倍而 `gpa` 几乎不变 → 证实是 cumulative 而非 per-semester
- client 不做语义翻译，dumb forward；`SemesterGpa.gpa` 是该学期截止的累计值，agent 自己理解

### sub_session client-fresh 但 server-dead 的盲点

- 5878fba 的 `cache_is_fresh(sub, main)` 只检查 client 端 `captured_at >= main.captured_at` 且 `!sub.is_expired()`（soft TTL 30d）
- **盲点**：ZF SP 自己的 server-side session TTL（实测 7+ 小时后失效），client 端 sub_session.json 仍 fresh 但 cookies 已被 server 弃用
- 失败现象：first call 报 `final_url=https://i.sjtu.edu.cn/xtgl/login_slogin.html`，提示 sub_session 过期或 CAS 链未走完
- 手动 workaround: `Remove-Item %APPDATA%\sjtu\sjtu-cli\config\sub_sessions\jwc.json` → cas_login 重做 → 拿新 cookies → ok
- **可能后续修**：检测 final_url 含 `login_slogin.html` 时自动 invalidate sub_session 文件 + 重试 1 次（属 T2.x 增强）

### data.rs 200 行硬限触底拆分

- `commands/jwc/data.rs` 已 200 行 → 拆出 `data/{mod, gpa}.rs`
- GPA 相关 5 个 struct（GpaData / GpaBySemesterData / SemesterGpa / SemesterFailure / SemesterKey + impl `From<&Gpa> for SemesterGpa`）一起搬到 `data/gpa.rs`
- mod.rs 用 `pub(in crate::commands::jwc) use gpa::{...}` re-export，外层 handler 调用零改动

---

## 2026-05-13 — T5 jwc 校历 iCal 导出

### 已发生的事故 + 修复

1. **T1 `#[expect(dead_code)]` 误判 pub 字段不触发 dead_code**
   - 现象：subagent 报告 "pub 不触发 dead_code"，主对话 verify 时撤掉 allow 后发现 dead_code 实际会触发
   - 根因：`#[expect(...)]` 在 binary crate + pub + Default + serde 构造的 struct 上不命中 "never constructed" lint
   - 修复：回退到 `#[allow(dead_code)]` + TODO 注释，等下游 task（T6）真消费这些字段时再清掉
   - 教训：lint expect 比 allow 更激进，对 serde-only 字段不可靠

2. **T2 multibyte fold 测试 pad 长度选错**
   - 现象：pad=70 让折行落在 ASCII 重复处，没测到 CJK 边界
   - 修复：pad=65，累计 73 字节后再跟 3 字节 CJK 字符，稳定触发折行；断言 `\r\n 操` 出现
   - 教训：RFC 5545 75-octet 折行的 UTF-8 安全测试要对准 octet 边界和多字节起点

3. **T4 events.rs 213 行 > 200 限**
   - 现象：subagent 报告 199 行，主对话 `wc -l` 实测 213 行
   - 根因：subagent 按 LF 计行，工作区实际是 CRLF
   - 修复：拆 `fnv1a_64` / `make_uid` 到 `uid.rs`（19 行），`events.rs` 降到 194 行
   - 教训：主对话必须亲跑 `wc` / `measure` 验证行数，不信口述

4. **T6 plan API path 错 10 处**
   - 现象：plan 中直接写 `crate::apps::jwc::api::*`（private mod），实际 `client.exams` 是 4 参且返回 `JwcPage<Exam>`
   - 修复：主对话在 brief 实装 subagent 时明确列出 10 处修正，统一走 re-export + `.items` 转 `Vec`
   - 教训：plan 到 80% 准确就够了，剩余偏差要在主对话 brief 时显式补差，不要假装 plan 已经 100% 精确

### 设计决策

- **FNV-1a 64-bit 手卷 hash** 代替 sha1 依赖：UID / envelope hash 只需要稳定去重，不需要密码学强度；16 字符 hex 足够
- **fail-soft 三路并发**：`tokio::join!` 并行拉课表 / 考试 / 学年校历；任一路失败只追加到 `warnings[]`，不阻塞其余两路
- **raw stdout + explicit envelope 双模**：不带 `--json` / `--yaml` 且不传 `--to` 时直接把 `.ics` 写 stdout（管道友好）；显式 `--json` / `--yaml` **或** 传 `--to` 时输出 envelope（任一即触发，`--to` 额外把 raw `.ics` 落盘），方便 agent 同时拿摘要和文件

---

## 2026-05-15 — T5 校历 iCal MVP T9 真机收尾

**真机 smoke 全过**：用户亲跑 Google Calendar / Apple Calendar / Outlook / 手机本地 4 端 import + 重复 import 幂等，全部通过。

### 真机新发现 + 已落地修正

1. **DTSTAMP 跨次刷新让文件级 hashHex 不稳定**
   - 现象：同 fixture 跑两次，bytes 完全一致但 hashHex 不同（`1c54f21acbcc4733` vs `c68c7ecc304b700f`）。`Compare-Object` 显示仅 `DTSTAMP` 行差异（实例化时间戳）；UID / DTSTART / DTEND / SUMMARY / RRULE 100% 跨次稳定。
   - 根因：RFC 5545 `DTSTAMP` 是 instance creation timestamp，每次跑必不同；FNV-1a 64-bit 算的是整份 `.ics` 字节，自然跟着变。
   - 已修：commit `c630284` 修 SKILL.md L141 措辞，明确 `hashHex` 仅供单次调用内 sanity check；跨次幂等保证靠 VEVENT 的 UID（基于学年/学期/类型/课号 FNV-1a 确定性）。客户端按 UID dedup。
   - 教训：写文档时把"hash 用于幂等比对"这种口语化描述当真，没核对算法实际包不包变量字段。RFC 5545 看一眼 `DTSTAMP` 定义即可避免。

2. **README / SKILL 三处描述与 handler 实际逻辑不一致**
   - 现象：T8 文档收尾时 README L76/L87 + SKILL L104 写"`--to` 仅额外落盘"，实际 `commands/jwc/ical/handler.rs:82` 是 `matches!(fmt, Json|Yaml) || to.is_some()` —— 即 `--json` / `--yaml` / `--to` **任一** 就触发 envelope 模式，stdout 改输 envelope；不带任何 flag 才把 raw `.ics` 写 stdout。
   - 根因：T8 plan 模板里的描述抄了一遍但没核对 handler 真实判定。
   - 已修：commit `f6ef916`（README L76/L87 + SKILL L104 三轨）。
   - 教训：纯文档 task 容易踩"应然 vs 实然"。下次写 envelope 行为描述前先 grep handler 入口看 `use_envelope` 实际判定式。

3. **iOS 微信打开 `.ics` 的分享菜单不带"日历"**
   - 现象：用户在 iOS 微信里点开 `.ics` → "..." → "分享"，菜单里没有"日历"应用图标。
   - 根因：微信预览界面的"分享"是微信内部分享（给好友），不走 iOS 系统级 share sheet；`.ics` → 系统日历的路径要求 UTI 派发到日历 app，必须经"用其他应用打开" / "存储到文件" / 邮件附件三条路径之一。
   - 替代路径（已给用户）：① 微信 → "..." → "存储到文件" → "文件" App 双击 `.ics` → 系统弹"添加 N 个事件"；② 邮件发自己，邮件 App 原生支持点附件直接加日历事件。
   - 教训：iOS "app 内分享" ≠ "OS 级 share sheet"。文档给用户 import 教程时要分清，别只写"分享 → 日历"这种依赖 share sheet 的路径。

4. **jwc sub_session 客户端 captured_at fresh 但 ZF 服务端已 timeout（staleness-fix 覆盖盲区）**
   - 现象：T9 第一次跑 `sjtu jwc calendar` `eventCount=0` + `warnings` 含 ZF redirect 到 `login_slogin.html`。客户端 `sub_sessions/jwc.json` captured_at 在 30 天窗口内（`cache_is_fresh ✓`），但 ZF 服务端 session 已 timeout（ZF 默认 30 分钟无活动）。
   - 临时修复（T9 内）：精准删 `%APPDATA%\sjtu\sjtu-cli\config\sub_sessions\jwc.json` 一个文件，让 `Client::connect` 用还有效的主 session（cookie 30 天）走 CAS 自动跳转刷新；**不动主 session** 避免用户重新扫码。删完第二次 `eventCount=16` / `warnings=[]`。
   - 根因：5878fba 之后的 staleness-fix 只在客户端 `captured_at` 上判断（`cas/mod.rs` / `oauth2/mod.rs` 都已 patch），但**没在 ZF redirect 检测路径上挂自动刷新**——服务端 TTL 30 分钟级，远短于客户端 cache TTL，必然漏。
   - 跟进 task（不阻塞 T5 收尾）：jwc HTTP 客户端在所有读端点封装层检测"返回 HTML / 跳转 `login_slogin.html`" → 自动删 `sub_sessions/jwc.json` + 重走 CAS 一次，失败再向上抛。同样思路应用到其他 CAS 子系统。
   - 教训：staleness 不能只看客户端 fresh，因为服务端 TTL 比客户端 cache 短的情况很常见。任何 reuse-then-fail 的链路都要做"用一次失败就刷一次"的 retry 包装，cache freshness 检查 + redirect 检测要双轨。

### 设计决策（追加）

- **跨次幂等靠 UID 不靠文件 hash**：UID 设计上对 `(xnm, xqm, kind, kch, ...)` 取 FNV-1a 是确定性的，跨次重跑稳定；hashHex 单次内可做 sanity（如外部篡改检测），跨次没意义

---

## 2026-05-13 — 水源写操作前必先确认分类（合规 + 不可补救）

**触发情境**：自动用 `sjtu shuiyuan new-topic` 发项目宣传帖，CLI 默认不传 `--category` → uncategorized → shuiyuan-bot 自动重路由 + 警告 reply。用户当面反馈"不合规"，要求记录。

**错误模式**：
1. 把"--category 是 Option<u64>"当成"可不传"，忘了 CP-W4 早就记录过 uncategorized 会被 bot 干预
2. 写端点的"二次确认"只看 `--yes` 是否传，没看 category 是否选；--yes 防的是误发，不是误分类
3. 站点 self-delete 422 + 首楼不可删 = 错分类后 CLI 无法补救，只能甩锅给 web UI

**正确做法**：
- 任何 `new-topic` / `reply` 前都先**对用户确认分类**，给候选列表（"程序员之家" / "学在交大" / "校园" / "闲聊" / etc.）
- CLI 端可考虑加 `shuiyuan list-categories` 子命令（一次 `/categories.json` 拉全量 id+name），或不传 `--category` 时 stderr 警告
- PM（pm-send）不受此规则约束（PM 没 category 概念）

**规则**：
- 水源写端点前一定问分类。不传 `--category` 默认走 uncategorized 是合规雷
- 任何"事后不可逆 + 默认配置可能错"的命令，CLI 层应当在确认提示里把默认值也呈现出来供用户拍板

**触发情境**：尝试 `sjtu shuiyuan new-topic` → 403 not_logged_in；`sjtu shuiyuan latest` 也 403。主 session captured_at=`2026-05-12`（昨天），shuiyuan.json captured_at=`2026-04-23`（19 天前）。soft_expires_at=30d 还未到 → OAuth2 path cache hit 直接复用，从未触发"重走 OAuth2"分支。Discourse 服务端的 `_t` token 已被 invalidate，但 client 端不知道。

**错误模式（之前埋下来的）**：
1. 5878fba 修 staleness 时只 patch `cas/mod.rs:cache_is_fresh` 一处，没同步 `oauth2/mod.rs`
2. 该函数的设计意图（"主重 login 后旧 sub 必须 stale"）是跨 auth 入口的通用语义，但实现散落在 CAS 模块内 + `pub(crate)` 可见而 OAuth2 module 没引用
3. lessons.md 当时（5878fba 期）也只记了 CAS path 的修法，没写"OAuth2 path 同等覆盖"这条 invariant

**正确做法**（本次修法）：
1. `oauth2/mod.rs` 把 `let main = load_session()` 提到函数顶（cache hit 也要拿 main 比 captured_at），把 `if !sess.is_expired()` 换成 `if cache_is_fresh(&sess, &main)`，复用 `cas::cache_is_fresh`
2. `oauth2/tests.rs` 加 2 个 wiring test：stale sub 必拒命中 / sub 比 main 新接受命中（与 cas/tests.rs 同构造范式）
3. 端到端验证：主 captured_at 不变情况下，shuiyuan.json captured_at 被新触发的 OAuth2 重做改写为运行时刻

**规则**：
- 新增任何 auth 入口（CAS / OAuth2 / 未来的 OIDC / SAML / 直拿 PAT 缓存…）的 cache hit 分支**必须**调用统一 staleness 函数（当前是 `cas::cache_is_fresh`），不可只看 `!sess.is_expired()`。这条规则进 reviewer checklist
- 共享 invariant 散落 + 没文档化 = 长期债。Staleness、redact、Envelope 这类语义有专门 lesson 卡死
- 修 bug 时不要只 patch 直接触发的入口，**枚举所有同类入口**一次性覆盖
- `pub(crate)` 函数被新模块使用时要在新模块加单测确认"被正确接入"（不是测函数本身，是测 wiring）

---

## 2026-05-17 — OAuth2 Authorization Code 手卷 vs `oauth2` crate

**触发情境**：T4 一卡通 OAuth2 spec 阶段需选实现路径。
**错误模式**：（潜在）直接引入 `oauth2` crate 似乎"省事"，但增加依赖面 + crate 默认开启 PKCE 与 SJTU 服务端兼容性未知。
**正确做法**：手卷 OAuth2 仅多 ~360 行（token.rs 114 + callback.rs 185 + authorize.rs 132），换零新依赖 + 完全可控的 state/PKCE 决策；spec OQ-1 留 PKCE 单独评估口。
**规则**：先评估"标准 crate 默认行为是否匹配我方 spec"，匹配再考虑引入；不匹配时手卷反而更少坑。

## 2026-05-17 — `headless_chrome::Browser` Drop 杀子进程

**触发情境**：T6 authorize.rs open_in_browser 让 chrome 弹出，等用户在浏览器同意。
**错误模式**：（潜在）spawn_blocking 闭包退出后 Browser drop → CDP 连接关闭 → chrome 进程被 OS 收 → 用户还没点同意浏览器就消失。
**正确做法**：`std::mem::forget(browser)` 故意泄露 ownership；trade-off：进程留到 CLI 退出。注释说明"必须跨过用户交互"。
**规则**：浏览器自动化里 Browser 持有者必须**跨过用户交互窗口**才能 Drop；用 `mem::forget` 是合法手段（不是泄露 bug）。

## 2026-05-17 — 第三方 API 字段拼写陷阱：`dateTimAccount`（少 1 e）

**触发情境**：T9 apps/card/models.rs 解 transactions entity，`orderBy=dateTimeAccount` 时返字段拼写漏 'e'。
**错误模式**：（潜在）看着像 typo 就自作主张改成 `dateTimeAccount`，反序列化静默失败该字段始终 None。
**正确做法**：`#[serde(rename = "dateTimAccount")]` 锁住服务端原拼写；Rust 字段名也照抄少 'e'（`date_tim_account_ms`）；module doc + 字段 doc 双处标注 "intentional typo mirror"。
**规则**：第三方 API 字段名以官方文档为准；宁可丑也别擦伤兼容；docs 显眼标注 typo intent，防止后人"修复"。

---

### 2026-05-18 — T4 一卡通 weixin path 双轨 fallback

**双轨架构动机**：OAuth2 client_id 审批长期阻塞（developer.sjtu.edu.cn 流程慢），需做"无 client_id 也能跑"的兜底。weixin path 复用网信中心已批的 `janicweixin20150709` client_id（用户主 jaccount cookie 透明跳 OAuth2），HTML scrape `weixin.sjtu.edu.cn` 拿余额 + 消费记录。

**Plan deviation 总览**（spec → plan 6 处 + plan → code 5 处共 11 条）：
- D1: drop `util::decimal_opt` — OAuth2 path 现有 `trans_balance: Decimal` 已覆盖 weixin 同义字段
- D2: `lost_status` / `freeze_status` enum **并行**保留 OAuth2 现有 bool 字段（不改 OAuth2 schema）
- D3: SCHEMA.md 新建（plan 假设已存在但项目根无）
- D4-D6: spec 误估 `BalanceData/HistoryData/TransactionItem` 字段名（实际 `card_no_redacted` 非 `card_no` 等）
- D7: `data_weixin.rs` 拆出（≤200 行硬限）
- D8: Task 10+11 commit 合并 atomic（避中间 cargo check 失败）
- D9: `handlers_dispatch.rs` 拆出（≤200 行硬限）
- D10: scraper 0.21 加 Cargo.toml（CLAUDE.md 项目结构早声明但实际依赖缺）
- D11: `HistoryData.card_no_redacted = "<weixin>"` 占位（weixin fetch_history 不返卡号；Agent 据 `meta.via` 判断）

**经验**：
1. **Spec 写"独有字段"时先 grep 现有 struct** —— 多次出现"以为是新字段，实际现有字段已覆盖"。spec 阶段 self-review 应含「字段唯一性扫描」。
2. **Plan 假设的 API/struct 字段名要现场 Read 校核**：plan deviation 4-6 全是 plan 阶段没 Read 现有 commands/card/data.rs，凭印象写字段名。下次 plan 阶段强制要求列「现有依赖 API 真实签名表」。
3. **200 行硬限触发拆分是常态** —— Task 9 / Task 10 都自动拆。implementer 直接执行拆分预案而非 BLOCK 主 agent 是合理判断。
4. **Envelope.meta 后向兼容设计**：`meta: Option<EnvelopeMeta>` + 内部字段 `Option<String>` + `skip_serializing_if`，5 个老子命令 JSON 输出形态 0 变化。
5. **Cookie struct 注入 reqwest jar**：`crate::cookies::Cookie` 是纯数据 struct 无 `to_set_str` 方法，手卷 `cookie_to_set_str(&Cookie) -> String` 拼 `name=value; Domain=; Path=` 喂 `Jar::add_cookie_str`。expires 不拼 —— Jar 不在乎，stale 由 `SubSessionStale` 信号驱动。
6. **`with_cas_refresh` 复用**：weixin path 复用 T8 的 retry helper，stale variant `SubSessionStale("card_weixin")` 由 `detect_stale_or_unexpected` 在响应 URL 落到 `jaccount/jalogin` 或 `oauth2/authorize` 时手动抛。
7. **PII 红线在 parse 层 enforce**：weixin balance_parse 主动 drop `姓名 / 学号 / 绑定银行卡` 行（不写入 CardInfo），不依赖上层 redact。

## 2026-05-20 — T8 邮箱 MVP（Zimbra SOAP）

### R12 — Zimbra SOAP `<context><authToken>` envelope 显式注入（不可省）

**结论**：Zimbra SOAP endpoint `POST /service/soap` **必须**在 envelope `<soap:Header>` 显式带：

```xml
<context xmlns="urn:zimbra">
  <authToken>{ZM_AUTH_TOKEN}</authToken>
</context>
```

**只放 cookie 不够** —— `ZM_AUTH_TOKEN` cookie 已带上、jaccount session 已跟链到 mail.sjtu.edu.cn，但 SOAP envelope 没 `<authToken>` 元素 → 服务端返 500 + Fault `service.AUTH_REQUIRED`，**完全不认 cookie 路径的 token**。

**Why**：Zimbra SOAP 接口设计上把 authToken 视作 envelope-level 凭据；cookie 路径只是 web UI（Zimlet）的 session 维系手段。两套互不通用。

**How to apply**：
1. 任何 Zimbra SOAP 子系统，第一步是 `extract_zm_auth_token(jar)` 从 reqwest jar 抠 cookie value
2. 用 `wrap_envelope(auth_token, body)` 包所有业务 envelope（search / get_msg / get_folder）—— **不要**在某个 builder 里漏带
3. 用 `is_auth_required_fault(xml)` substring 匹配 `<Code>service.AUTH_REQUIRED</Code>`，识别为 `SjtuCliError::SessionExpired`，触发 stale 路径

### R13 — `read="0" html="0" max="50000"` 编译期注入红线（用户层零开关）

**结论**：`GetMsgRequest` 的三个属性必须在 `build_get_msg_envelope(auth_token, msg_id)` builder 内部**硬编码**：

```rust
format!(r#"<m id="{msg_id}" read="0" html="0" max="50000"/>"#)
```

**为什么不暴露开关**：
- `read="0"`：**绝不**标已读（user 通过 CLI 读邮件不应该影响"未读"状态，否则用户回邮箱客户端看会困惑）
- `html="0"`：**绝不**取 HTML（agent 用，HTML 解析复杂 + 引入 XSS / phishing 链接信号污染；用户要看 HTML 自己开邮件客户端）
- `max="50000"`：体积上限 50KB（防 dump 大附件 dataURL；超出走 `body_warning` 提示）

**How to apply**：任何"用户行为相关的关键安全属性"都应该在 builder/struct 创建处编译期硬定，不暴露 CLI flag。CLI flag 暴露 = 用户/agent 误开一次就破红线。

### R14 — IMAP 路线放弃：jaccount master password 不可代输

**结论**：IMAP 连接 sjtu 邮件需要 jaccount 完整账号 + master password（不能用 SSO token）；CLI 设计**红线**之一是"不代用户输入 jaccount 密码"——所以 IMAP 路线在 L0 调研阶段就被排除。

**Why**：
- jaccount master password 是高敏感凭据，CLI 持有 = 单点风险
- 我们已有 jaccount **session cookie** 透传机制（QR 扫码登录 → cookies 落盘 600 权限）
- Zimbra SOAP 路线只需 session → 跟链 → ZM_AUTH_TOKEN，**完全无需 master password**

**How to apply**：新增 SJTU 子系统 L0 阶段，先验证"是否能用现有 jaccount session SSO 进去"。如果只能走账号密码 / OAuth client_secret 路径，**默认放弃**（红线）；如果走 client_id-only（如 card OAuth2 PKCE），单独评估。

### R15 — plan 字面与项目现状脱节时，跟随**现状**（implementer 不替 controller 决策）

**结论**：plan 字面是 brainstorming 时的形状预设，未必准确反映项目 evolution。R5 review 发现 plan 字面与现状两处冲突：

| 项 | plan 字面 | 现状（grep 7 个子系统） | 解决 |
|---|---|---|---|
| handlers 签名 | `Result<Envelope<T>>` 由 dispatch 处 render | `Result<()>` + 内部 `render(envelope, fmt)`（所有 cmd_*）| 跟随现状 |
| cli enum 包装 | `MailArgs { sub: MailSub }` | `pub enum XxxSub` 直接（library/card/jwc 全是）| 跟随现状 |

**Why**：plan 写时 controller 未必逐子系统 grep 实际签名，可能凭印象写。implementer 若机械执行 plan 字面，会引入"7 子系统 1 个特例"的 refactor 噪音 —— 而这种 refactor 没有 controller 拍板，是 implementer 越权决策。

**How to apply**：
1. implementer Pre-Step 0 **强制 grep 现有 `cmd_*` 函数签名 + `pub enum.*Sub`** 作 ground truth
2. 若 plan 字面与现状冲突：**跟随现状**，在 self-review 段落显式声明 deviation reason "plan 字面错而非 implementer 错"，让 reviewer 判
3. reviewer 应该把这种声明当作"plan 错请 controller ACK"信号，而**不是**"implementer 偷懒"
4. controller ACK 后回头补 plan 校正（如果后续还要参考该 plan）

**反例**：implementer 严格跟 plan 字面 → 写出 7 子系统中唯一一个 `Result<Envelope<T>>` 签名的 mail handler → reviewer 通过 → 后续 mail 调用 dispatch 时签名不匹配 → 临 commit 才发现要全 refactor 或 wrap，纯浪费 token。

