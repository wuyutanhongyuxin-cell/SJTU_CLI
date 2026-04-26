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

<!-- 新的经验追加到此处上方，最新在上 -->
