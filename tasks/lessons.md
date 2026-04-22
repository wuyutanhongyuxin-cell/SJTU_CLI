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
