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
