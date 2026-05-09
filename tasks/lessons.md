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

<!-- 新的经验追加到此处上方，最新在上 -->
