# i.sjtu 调研规格表（半自动 chrome-devtools MCP 抓取）

> 起点 2026-04-26。`i.sjtu.edu.cn` = 上海交大教务系统（教学管理信息服务平台），底座 = ZFSOFT 正方教务（Tomcat 7.0.94 + Java 1.8 + Spring + Velocity 风格 URL）。
>
> 调研模式：CLAUDE.md 硬红线 + `feedback_isjtu_semiauto.md`（用户点击 / 我抓 network）。
>
> **本文件只记录字段定义、接口形态、鉴权链；不存任何真实学号 / 姓名 / 成绩数据**。

---

## 1. 通用范式（全 SP 共享）

### 1.1 nav → SP 映射

主入口 `https://i.sjtu.edu.cn/xtgl/index_initMenu.html?jsdm=xs` 已登录态返回完整 nav HTML；每个 SP 用 `clickMenu(gnmkdm, path, title, p)` JS 跳转。CLI 直接拼 URL：

```
https://i.sjtu.edu.cn<path>?gnmkdm=<gnmkdm>&layout=default
```

`gnmkdm` 是功能模块代码（`N\d{4,6}`），全清单见 §3。

### 1.2 鉴权链

```
jaccount QR / CAS  ──►  i.sjtu.edu.cn JSESSIONID + keepalive cookie
                          ▲
                          └ 已有 src/auth/cas/ 模块覆盖（S2 已验，缓存命中 6ms）
```

cookie 字段：
- `JSESSIONID` — Tomcat session，HttpOnly，**关键**
- `keepalive` — 服务端定期刷新，每次 200 响应都 set-cookie，CLI 的 reqwest cookie store 自动接住即可
- `csrftoken`（**注意**：在 page HTML 的 `<input type=hidden>` 而非 cookie；form 提交时随 body 走）—— 双段拼接 `<full-uuid>,<hex22>`

**所以 jwc API 不需要单独 GET csrf 端点**，只在写操作时从主页面 HTML 解析（CLI 暂不实现写）。读操作只要 JSESSIONID 有效即可。

### 1.3 GET-via-POST 模式

ZF 系统**全部数据查询都用 POST**（即使语义只读），URL 形如 `xxx_cxXxx.html?doType=query&gnmkdm=<gnmkdm>`。
- `doType=query` 拉数据
- `doType` 还可能是 `details` / `print` 等，按 SP 而定
- query string 必带 `gnmkdm`，去掉会被 redirect 到 nav

所有数据响应都是统一分页 envelope：

```json
{
  "currentPage": 1, "pageNo": 0, "pageSize": 15, "showCount": 15,
  "totalCount": 0, "totalPage": 4, "totalResult": 52,
  "items": [ /* 实体列表 */ ],
  "currentResult": 0
}
```

### 1.4 必备 HTTP headers

```
Content-Type:    application/x-www-form-urlencoded;charset=UTF-8
Accept:          application/json, text/javascript, */*; q=0.01
X-Requested-With: XMLHttpRequest        # 不带可能被路由到 HTML 兜底
Referer:         https://i.sjtu.edu.cn/<page>?gnmkdm=...   # 部分端点校验
User-Agent:      <真实浏览器 UA>          # 缺会触发 WAF
Origin:          https://i.sjtu.edu.cn   # POST 必带
```

### 1.5 公共 form 字段

```
queryModel.showCount      = 15..500 （分页大小，下拉提供 15/20/30/50/70/90/100/150/300/500）
queryModel.currentPage    = 1, 2, ...
queryModel.sortName       = "+"        （默认；少数 SP 可以传字段名）
queryModel.sortOrder      = "asc" | "desc"
_search                   = "false"
nd                        = unix ms ts （防缓存戳）
time                      = 自增 int   （同一会话内查询次数；从 0 起）
pkey                      = ""         （多数 SP 留空）
```

---

## 2. SP 已验规格

### 2.1 N305005 — 学生成绩查询 ✅ 2026-04-26 真机抓

**端点**

```
POST /cjcx/cjcx_cxXsgrcj.html?doType=query&gnmkdm=N305005
```

**form 专属字段**

| 字段 | 值域 | 含义 |
|---|---|---|
| `xnm`    | 4 位学年码 / 空 | 学年（e.g. `2025`=2025-2026），空=全部 |
| `xqm`    | `3` / `12` / `16` / 空 | 学期 1 / 2 / 3，空=全部 |
| `sfzgcj` | 空 / `1` | 是否仅最高成绩 |
| `kcbj`   | 空 / `0..4` | 主修/辅修/二专业/二学位/非学位 |

**item 字段（CLI 暴露 vs 内部冗余）**

CLI 暴露（→ `models::Grade`）：
```
xnmmc        → academic_year     "2023-2024"
xqmmc        → semester          "1" / "2" / "3"
kch          → course_code       "FL1405"
kcmc         → course_name       中文名
kcywmc       → course_name_en    英文名
xf           → credit            String → Decimal
cj / bfzcj   → score             "86" / 等级（"考核"科目可能给字母）
jd           → gpa_point         Decimal
xfjd         → weighted_gpa      Decimal（= xf * jd）
kcxzmc       → course_type       "必修" / "限选" / "通识核心课程"
kcbj         → curriculum        "主修" / "辅修" / ...
khfsmc       → assessment        "考试" / "考核"
jsxm         → instructor        多教师以 ";" 分隔
kkbmmc       → department
jxbmc        → class_name        "(2023-2024-1)-FL1405-01"
cjbdsj       → graded_at         "YYYY-MM-DD HH:MM:SS"
```

冗余字段（CLI 不暴露 / 仅内部 join 用）：
- `bh`/`bh_id`/`bj` — 班级
- `jxb_id`/`kch_id`/`zyh_id`/`jg_id` — ZF 内部主键
- `xh_id` — **注意**：长 256 hex 字符，疑似带签名的 token，不是 raw 学号；不要往日志写
- `userModel`/`pageTotal`/`queryModel`（嵌套了一份外层分页 envelope，是 ZF 怪癖，忽略）
- `date`/`dateDigit`/`dateDigitSeparator`/`day`/`month`/`year` — 响应时间冗余，**与成绩本身无关**
- `localeKey`/`listnav`/`pageable`/`rangeable`/`row_id` — 显示侧

**已知坑**
- 当前学期成绩未公布时，`xnm=<当前学年>&xqm=<当前学期>` 返回 `items:[], totalResult:0`（不是错误）
- `totalResult` 字段是**字符串**（"52"），不是 int —— 反序列化要么用 `String` 要么自定义 `serde(deserialize_with)`
- `cj` 对"考核"型课程可能是字母 / "通过"，不是数字 —— 不要 force parse 成 `f64`

---

### 2.2 N2151 — 个人课表查询 ✅ 2026-04-26 真机抓

**端点（页面加载自动调，无需点查询按钮）**

```
POST /kbcx/xskbcx_cxXsgrkb.html?gnmkdm=N2151
```

页面加载时**还自动调了**几个旁路接口（CLI 不需要）：
- `POST /xssygl/sykbcx_cxSykbcxxsIndex.html?doType=query&gnmkdm=N2151` — 学生时空（实验/实践课表）
- `POST /jssygl/sykbcx_cxKfxSykbcxIndex.html?doType=query&gnmkdm=N704551` — 教师可访学生时空
- `POST /kbdy/bjkbdy_cxKbzdxsxx.html?gnmkdm=N2151` — 课表自定义显示项
- `POST /kbcx/xskbcx_cxRsd.html?gnmkdm=N2151` — 闰时段（如有）
- `POST /kbcx/xskbcx_cxRjc.html?gnmkdm=N2151` — 闰节次（如有）

**form 字段（极简）**

```
xnm=<学年4位>      e.g. 2025（默认 = 当前学年）
xqm=<学期>         e.g. 12（默认 = 当前学期）
kzlx=ck            固定常量（kzlx=控制类型, ck=查看；不传会被打回）
xsdm=              空
kclbdm=            空
kclxdm=            空
```

**响应 envelope**（**不是**标准 §1.3 分页 envelope，课表用专属结构）

```json
{
  "qsxqj":  "1",
  "xsxx":   { /* 学生身份冗余：xh/xm/xnm/xqm/xqmmc/zymc/njdm_id... — CLI 一律脱敏 */ },
  "sjkList": [],
  "sjfwkg": true,
  "xqjmcMap": { "1":"星期一", ..., "7":"星期日" },
  "rqazcList": [],
  "xskbsfxstkzt": "0",
  "kbList": [ { /* 每条课程一项 */ } ]
}
```

**kbList[*] 字段（CLI 暴露）**

| 字段 | 含义 |
|---|---|
| `xnm` / `xqm` | 学年 / 学期代码 |
| `kch` / `kcmc` | 课程号 / 课程名 |
| `xf` | 学分（字符串 → Decimal） |
| `kcxz` | 课程性质（"必修" / "限选" / "任选"） |
| `kclb` | 课程类别（"专业类教育课程" / "通识教育课程" / 等） |
| `khfsmc` / `ksfsmc` | 考核方式 / 考试方式（"考试" / "考核"，"笔试" / "大作业"） |
| `jxbmc` | 教学班名 `(YYYY-YYYY-N)-<kch>-<bjh>` |
| `jxbzc` | 教学班组成（"2023日语" / 多专业 ";" 分隔） |
| `xm` / `zcmc` / `zfjmc` | 教师姓名 / 职称 / 主讲身份 |
| `xqj` / `xqjmc` | 周几（1..7 / "星期一".."星期日"，配合 `xqjmcMap`） |
| `jc` / `jcs` / `jcor` | 节次显示 / 节次范围 |
| `zcd` | 周次描述（"1-16周" / "1-4周,6-16周"，**逗号分隔，需 parser**） |
| `cdmc` / `lh` / `cdlbmc` / `xqmc` | 教室 / 楼 / 教室类型 / 校区 |
| `skfsmc` | 授课语言（"中文" / "英文"） |

**冗余字段**（CLI 不暴露）：`bklxdjmc / cxbj / cxbjmc / oldjc / oldzc`(疑位 mask) / `jgh_id / jxb_id / kch_id / xkbz / cd_id / cdbh / xqh_id / xqh1` / `date / dateDigit / day / month / year`(响应时间) / `queryModel / userModel / pageTotal / pageable / rangeable / row_id / listnav / localeKey / px / sxbj / sfjf / sfkckkb / kkzt / kklxdm / pkbj / xkrs / zzrl / zzmm / zyfxmc / xsdm / xslxbj / zyhxkcbj / xkbz / qqqh / rk / rsdzjs / njxh / zxs / zxxx / zhxs / kcxszc / kczxs / totalResult`（嵌在每条 item 里 = "0"，**ZF 怪癖**）

**已知坑 + 解析建议**
- `kbList` 是**已经按周几+节次铺平**的数据，CLI 拿到直接按 `xqj * 100 + jcor.split('-')[0]` 排序就能拼周课表
- `zcd` 字符串需自写 parser（"1-16周" → 1..=16，"1-4周,6-16周" → [1..=4, 6..=16]，"2-16周双" 偶周）
- `xsxx` 对象包含 `XH / XM / YWXM / NJDM_ID / ZYMC / BJMC` —— **全是个人身份**，CLI Envelope 默认全部抹掉，仅当 `--with-identity` 才输出
- N2151 = 学年学期视图（一周完整 7 天 7 节）；想按周次查 → 走 N2154（路径 `/kbcx/xskbcxZccx_cxXskbcxIndex.html`）
- 页面加载就自动调，**没有"查询"按钮触发的二次拉**（与 N305005 不同）—— CLI 端直接 POST 不需要先 GET 页面

---

### 2.3 N309131 — GPA / 学积分查询 ✅ 2026-04-26 真机抓

**两阶段流程**（页面没有"查询"按钮，按钮叫"统计" id=`btn_tj`；点一次会触发两个连续 POST）：

```
1) POST /cjpmtj/gpapmtj_tjGpapmtj.html?gnmkdm=N309131           ← 触发 server-side 计算（写临时表）
   response body: "统计成功！"   （**注意**：是裸 JSON 字符串，不是对象）

2) POST /cjpmtj/gpapmtj_cxGpaxjfcxIndex.html?doType=query&gnmkdm=N309131   ← 拉计算结果
   response body: 标准 §1.3 分页 envelope，items[0] 是当前学生
```

**form 字段**（两阶段共享同一组 + step2 加 queryModel.\*）

| 字段 | 含义 / 默认 |
|---|---|
| `qsXnxq` / `zzXnxq` | 起止学年学期，6 位编码 `<学年4位><学期2位>`，e.g. `202903` = 2029-2030 第 1 学期；空 = 全部 |
| `tjgx` | 统计规则（默认 `0`） |
| `alsfj` | 案例是否记（空） |
| `sspjfblws` / `pjjdblws` | 加权平均分 / 绩点保留位数（默认都 `9`） |
| `bjpjf` | **不计平均分**的成绩类型，逗号串：`"缓考,缓考(重考),尚未修读,暂不记录,中期退课,重考报名"` |
| `bjjd`  | **不计绩点**的成绩类型，同 `bjpjf` 默认 |
| `kch_ids` | **排除课程 ID** 列表，e.g. `MARX1205,TH009,TH020,FCE62...DCCC`（思政/体育默认排除） |
| `bcjkc_id` / `bcjkz_id` / `cjkz_id` | 不计算课程 / 课组 / 计算课组（默认空） |
| `cjxzm` | 成绩选择模式（默认 `zhyccj`=综合应用最终成绩） |
| `kcfw` | 课程范围（`hxkc`=核心课程 / `qbkc`=全部课程） |
| `tjfw` | 统计/排名范围（`njzy`=年级专业 / 还有 `nj`=年级 / `bj`=班级 等） |
| `xjzt` | 学籍状态（默认 `1`=在籍） |

step 2 额外公共字段：`_search=false / nd=<ts> / queryModel.showCount / queryModel.currentPage / queryModel.sortName=+ / queryModel.sortOrder=asc / time=<int>`

**items[0] 字段（CLI 暴露）**

| 字段 | 含义 |
|---|---|
| `gpa` / `gpapm` | GPA / GPA 排名 `"X/Y"`（自身/总数）|
| `xjf` / `xjfpm` | 学积分（加权平均分） / 学积分排名 |
| `zf` | 总分（成绩加总） |
| `ms` | 总门数 |
| `bjgmc` / `bjgms` | 不及格门次 / 不及格门数（**不同！**门次=同课多次都计，门数=去重） |
| `zxf` / `hdxf` / `bjgxf` | 总学分 / 获得学分 / 不及格学分 |
| `tgl` | 通过率（"100%" 字符串带百分号） |
| `kcfw` | 回显请求里的课程范围 |
| `czsj` | 统计计算时间 `YYYY-MM-DD HH:MM:SS` |
| `xh` / `xm` / `bj` / `jgmc` / `zymc` / `njmc` | 身份冗余 → 默认脱敏 |

**冗余**：`pm1`/`pm2`(分项排名内部用) / `tj_id`(临时表 PK，server 内部) / `date/dateDigit/day/month/year` / `jgpxzd / listnav / localeKey / queryModel / userModel / pageable / rangeable / row_id / totalResult` / `xh_id`(此 SP 给的是 raw 学号，与 N305005 给 256-hex token 不同，仍按身份脱敏)

**已知坑 + 实现建议**
- step 1 返 `"统计成功！"` 不是 JSON 对象 —— reqwest `.json::<String>()` 可以直接吃掉裸字符串；如果失败要降级 text 比对
- 如果**直接调 step 2 跳过 step 1**，server 会返空 / 报错 ——必须按"先统计再查"的两步序列
- 默认条件已合理（核心课程 + 年级专业排名），CLI MVP 直接复用 form 默认值即可；高级 flag 留 `--scope qbkc|hxkc` / `--rank njzy|nj|bj`
- `bjpjf` / `bjjd` 是**字符串列表**而非数组 —— CLI 端硬编码默认值即可（除非用户改）

---

### 2.4 N358105 — 考试信息查询 ✅ 2026-04-26 真机抓

**端点**（页面加载自动调一次默认学期；标准单阶段查询）

```
POST /kwgl/kscx_cxXsksxxIndex.html?doType=query&gnmkdm=N358105
```

**form 字段**

| 字段 | 含义 |
|---|---|
| `xnm` | 学年 4 位（默认当前学年） |
| `xqm` | 学期 `3`/`12`/`16`（默认当前学期）|
| `ksmcdmb_id` | 考试名称代码 ID（页面下拉提供 ID 列表，如"2025-2026-2 期末考试"），空 = 全部 |
| `kch` | 课程号过滤（空） |
| `kc` | 课程名过滤（空） |
| `ksrq` | 考试日期过滤（空 / `YYYY-MM-DD`） |
| `kkbm_id` | 开课部门 ID 过滤（空） |
| 公共 | `_search / nd / queryModel.* / time` |

**响应**：标准 §1.3 分页 envelope，`items[]` 每条为一场考试。

**items[*] 字段（CLI 暴露）**

| 字段 | 含义 |
|---|---|
| `xnm` / `xnmc` / `xqm` / `xqmmc` | 学年码 / 学年名 / 学期码 / 学期名 |
| `ksmc` | 考试名（`"YYYY-YYYY-N期末考试"` / `"...期中考试"` / `"...免修考"`） |
| `kssj` | 考试时间字符串（**`"YYYY-MM-DD(HH:MM-HH:MM)"`** 复合，CLI 端拆 date+开始时刻+结束时刻） |
| `kch` / `kcmc` | 课程号 / 课程名 |
| `jxbmc` / `jxbzc` | 教学班名 / 教学班组成 |
| `xf` | 学分（字符串） |
| `khfs` / `ksfs` | 考核方式 / 考试方式（"考试" / "笔试" / "大作业"） |
| `cdmc` | **考场**（"上院412"），≠ `jxdd`（=上课教学地点） |
| `cdbh` | 考场编号（"SY412"） |
| `cdxqmc` | 考场校区（"闵行"） |
| `kkxy` | 开课学院 |
| `jsxx` | 监考教师，`<工号>/<姓名>` 复合 |
| `sjbh` | 时间安排编号 `"学校统一-<ksmc>-<kch>"`（内部安排标识，CLI 可作为去重键） |
| `cxbj` | 是否补考（"是"/"否"） |
| `pycc` | 培养层次（"本科"） |

**冗余 / 身份字段（默认脱敏）**

`xh / xh_id / xm / xb / bj / njmc / jgmc / zymc / sksj`(上课时间冗余) / `jxdd`(上课地点 ≠ 考场) / `cdjc`(空缩写) / `totalresult / row_id / zxbj`

**已知坑**
- `kssj` 是字符串而非分立 date/start/end —— CLI 抽 `parse_kssj("2024-06-18(10:30-12:30)") -> (NaiveDate, NaiveTime, NaiveTime)`
- 当前学期未排考时返 `items:[], totalResult:0` 不报错；建议 CLI 默认 `--term latest-graded` 选最近一个有考试的学期，避免开学初查空
- `cdmc` 和 `jxdd` 都是地点字符串但**含义不同**：`cdmc=考场`、`jxdd=上课地点`（CLI 显示考场用 `cdmc`）
- `jsxx` 含监考教师工号 + 姓名，脱敏时按 `工号` 截掉前缀（含工号属个人身份）

---

### 2.5 N305007 — 学生成绩明细查询（master-detail）✅ 2026-04-26 真机抓

**与 N305005 区别**：N305005 给的是**总评**一行；N305007 给**项目分**（平时分 / 期中 / 期末 / 实验等加权小项）。Master = 课程列表，Detail = 该课程的小项分列表。

**两个端点**

```
1) MASTER  POST /cjcx/cjcx_cxXsKcList.html?gnmkdm=N305007
   form:   xnm=<学年>&xqm=<学期>&queryModel.* 等公共字段
   返回：标准分页 envelope，items[] = 该学期所修课程

2) DETAIL  POST /cjcx/cjcx_cxXsKccjList.html?gnmkdm=N305007
   form:   jxb_id=<master 行的 jxb_id>&xnm=<>&xqm=<>&queryModel.* 等公共字段
   返回：标准分页 envelope，items[] = 该课程的小项分（一项一行）

合并视图 (Tab"课程成绩合并显示")  POST /cjcx/cjcx_cxDgXsxmcj.html?doType=query&gnmkdm=N305007
   —— 等价 master + detail 一次性铺平，但每条 item 仍是单小项（不是聚合）；CLI 端按 master+detail 两阶段更可控
```

**Master items[*] 字段（CLI 暴露）**

| 字段 | 含义 |
|---|---|
| `xnm` / `xnmmc` / `xqm` / `xqmmc` | 学年/学期 |
| `kch` / `kcmc` / `kch_id` | 课程号 / 课程名 / 课程内部 ID |
| `jxb_id` / `jxbmc` | 教学班 ID（**关键 join key**） / 教学班名 |
| `kkbm_id` / `kkbmmc` | 开课部门 |
| `xf` | 学分（字符串） |
| `zpcj` | **总评成绩**（与 N305005 的 `cj` 同语义） |

**Detail items[*] 专属字段（除 master 字段冗余外）**

| 字段 | 含义 |
|---|---|
| `xmblmc` | **项目类别名**（"平时(50%)" / "期中(30%)" / "期末(50%)" / "实验(20%)" 等，**含权重百分号**） |
| `xmcj`   | **项目成绩**（"81" / 字母 / "通过"，与 N305005 `cj` 同处理：不要 force parse） |

**已知坑**
- master items 不带 `cj` / `bfzcj` 字段，**只有 `zpcj` 总评** —— 与 N305005 字段名不一致，CLI 反序列化两者用不同 struct
- detail 的 `xmblmc` 含中文括号 + 百分号 —— CLI 解析权重时正则 `(\d+)%`，找不到就当 `xmblmc` 整段当 label 不解
- 默认学年学期为当前 = 多半空（学期未结）；CLI MVP 默认查"上学期"或让用户传 `--year/--term`
- master/detail 都是**标准 §1.3 envelope**，不是 N2151 那种专属结构 —— 复用 `JwcPage<T>` 即可

---

### 2.6 N551225 — 学生修业情况查询（毕业资格审核）✅ 2026-04-26 真机抓

**SP 特点**：3 层结构（一级模块 → 二级子模块 → 三级类别 + 总分），每个**三级节点**对应一个 `xfyqjd_id`，需 1 + N 调用拉全。CLI 端 MVP 出 **overview** 即可（已含每模块的"要求/已得"学分 + 达标标志），detail 钻取留给 `--detail`。

**端点**

```
1) OVERVIEW  POST /xjyj/xsxyqk_ckXsXyxxHtmlView.html?doType=query&xh_id=<学号>&gnmkdm=N551225
   注意：xh_id 拼在 URL 而非 form ——CLI 要先 GET 主页面 parse 出 xh_id 再调
   form: 极简公共字段（_search=false / nd / queryModel.showCount=-1 表示不分页 / time=0）
   response: 标准分页 envelope，items[] = 各三级节点

2) DETAIL   POST /xjyj/xsxyqk_ckDynamicGridData.html?doType=query&xh_id=<>&xfyqjd_id=<>&gnmkdm=N551225
   每个 overview item 一调，N 次（≈ 20 次）；form 同 overview
   response: 标准分页 envelope，items[] = 该节点下已修课程
   特殊 xfyqjd_id：`gxhkcxdqk` = 个性化课程修读情况（不是 UUID，是固定常量）

3) 旁路     POST /xjyj/xsxyqk_cxYyspAndTycj.html?gnmkdm=N551225 — 应用申请审批 + 体育成绩，CLI 暂不需要
```

**OVERVIEW items[*] 字段**

| 字段 | 含义 |
|---|---|
| `level2` / `level2_id` | 一级模块（"通识教育课程" / "专业教育课程" 等），**值含 `<a href>` HTML 标签需 strip** |
| `level3` / `level3_id` | 二级子模块（"公共课程类(26 / 23)"），同样含 HTML |
| `level4` / `level4_id` | 三级节点（"必修" / "总分" / "数学选修"） |
| `xfyqjd_id` | **关键 join key**：detail 用此 ID 拉课程列表 |
| `yqzdxf` | 要求最低学分 |
| `hdxf` | 已得学分 |
| `hmxf` / `hmxf_ts` | 豁免学分 / 豁免学分（转换） |
| `zgshzt` | **资格审核状态**：`"Y"` = 达标 / `"N"` = 未达标 |
| `rn` | 行号（overview 内部排序用） |

**DETAIL items[*] 字段**

| 字段 | 含义 |
|---|---|
| `kch` / `kcmc` / `kkbmmc` | 课程号 / 课程名 / 开课院系 |
| `kcxzdm` | 课程性质代码 |
| `xf` | 学分（字符串） |
| `cj` / `bfzcj` / `yjcj` | 成绩 / 百分制 / 原始成绩；`cj` 可为 `"P"`(Pass) / `"W"`(Withdrawal) / 数字 |
| `jyxdxnm` / `jyxdxnmc` / `jyxdxqm` / `jyxdxqmc` | 修读学年 / 学年名 / 学期 / 学期名 |
| `zkxnxqmc` | 主考学年学期 `"YYYY-YYYY-N"` |
| `xdzt` | 修读状态码（`"21"` = 已修，`"0"` = 尚未修读，未做完整反查） |
| `cjlybj` | 成绩来源标识（`"cjb"` = 成绩表） |
| `xfyqjd_id` | 反指 overview 节点 |
| `bzxx` | 备注信息（数字编码，反查表未做） |

**已知坑**
- overview `level2/3/4` 字段是 HTML 字符串 `<a href="#xxx">名称</a>(yqzdxf / hdxf)` —— CLI 端 regex 提取标签内文本，或 fallback 用 `level2_id` join 反推显示名
- `xh_id` 必须**拼在 URL** 不能放 form —— ZF 异常设计，CLI 端构造时格外注意
- `queryModel.showCount=-1` = 不分页（取全部） —— CLI 端不需要再分页循环
- 个性化课程节点 `xfyqjd_id=gxhkcxdqk` 是字符串常量，不是 UUID
- detail 的 `cj="W"` 表示中期退课（Withdrawal），CLI 显示时注意分流（"P"/"W"/字母/数字）

---

### 2.7 N2154 — 学生课表查询（按周次） ✅ 2026-04-26 真机抓

**与 N2151 区别**：N2151 一发拉学年学期所有课，CLI 自己分周；N2154 多了**周次参数 `zs`** + 返回 `rqazcList[]` 当周日期映射（**N2151 此字段为空**）。

**端点**

```
POST /kbcx/xskbcxMobile_cxXsKb.html?gnmkdm=N2154
旁路：
  POST /kbcx/xskbcxMobile_cxRsd.html?gnmkdm=N2154   # 闰时段
  POST /jzgl/skxxMobile_cxRsdjc.html?gnmkdm=N2154   # 闰节次
```

**form 字段**

```
xnm=<学年>&xqm=<学期>&zs=<1..18>&kblx=1&doType=app&xh=
```

- `zs` = 周次（**关键**，1..教学周末）
- `kblx=1` = 课表类型 1（学生课表）
- `doType=app` = 移动端样式（Mobile 端口共享）
- `xh=` 留空 = session 自动取登录态用户

**response 结构**

完全沿用 N2151 的 `xsxx / sjkList / xqjmcMap / kbList / xskbsfxstkzt` 字段，但 **`rqazcList[]` 不为空**：

```json
"rqazcList": [
  { "xqj": 1, "rq": "2026-04-20" },
  { "xqj": 2, "rq": "2026-04-21" },
  ...
  { "xqj": 7, "rq": "2026-04-26" }
]
```

= 第 `zs` 周每天 ISO 日期；CLI 直接用 `xqj → rq` 把课程映射到公历日。

**重要发现：`oldzc` 是 16-bit 周次掩码**

每条 `kbList[*]` 都有 `oldzc` 数值字段：
- `"1-16周"` → `oldzc = 65535` （0xFFFF，全 16 位置 1）
- `"1-2周,4-16周"` → `oldzc = 65531` （0xFFFB，仅第 3 位为 0）
- `"1-4周,6-16周"` → `oldzc = 65519` （0xFFEF，仅第 5 位为 0）

CLI 实现：判断"该课是否在第 N 周上" → `(oldzc >> (N - 1)) & 1 == 1`，**比 parse `zcd` 字符串可靠得多**。MVP 周课表 = N2154 单发 + 客户端 mask 过滤即可。

**`oldjc` 同理 = 节次掩码**

`oldjc` 也是 bitmask，对应"该课跨哪几节"：`oldjc = 12` = 0b1100 = 第 3-4 节；`oldjc = 192` = 0b11000000 = 第 7-8 节；`oldjc = 768` = 0b1100000000 = 第 9-10 节；`oldjc = 3` = 0b11 = 第 1-2 节。CLI 用这个比 parse `jc / jcor` 字符串干净。

**已知坑**
- N2154 response 不会自动过滤"本周不上的课"，CLI 必须自己用 `oldzc` mask
- `kblx` 还可以是其他值（教师课表 / 班级课表），但学生 CLI 固定 `1`
- `rqazcList` 长度固定 7（一周）；周末没课时仍返回该周日期

---

### 2.8 N153521 — 培养计划课程查询 ✅ 2026-04-26 真机抓（含 N153540）

**端点**

```
POST /jxzxjhgl/pyjhkcxxcx_cxPyjhkcxxIndex.html?doType=query&gnmkdm=N153521
旁路：POST /kkqkcx/kkqkcx_cxKcxsxx.html?gnmkdm=N153521  # 课程开课情况字典
```

**form 字段**（用户身份不在 form 里，**全校所有培养计划默认全捞**）

```
jyxdxnm=<学年>&jyxdxqm=<学期 3/12/16>
njdm_id=<年级>            # 空 = 全部
jg_id=<学院>              # 空 = 全部
kkbm_id=<开课部门>         # 空 = 全部
kkxy_id=<开课学院>         # 空 = 全部
zyh_id=<专业代码>          # 空 = 全部 ←← CLI 必填以过滤自身专业
kch=<课程号关键字>         # 空 = 全部
ksxsdm=<考试形式>          # 空 = 全部
kcxzdm=<课程性质代码>      # 空（01=必修，02=限选，07=个性化）
kslbdm=<课程类别代码>
kkzt=<开课状态>            # 空（1=正常）
xdlx=<修读类型>            # zx=主修
+ 公共：_search/nd/queryModel.*/time
```

**默认无过滤时返 totalResult ≈ 412 条**，含全校所有培养计划课程（涵盖各专业）。CLI **必须自带 `zyh_id` 过滤**。

**items[*] 字段（CLI 暴露）**

| 字段 | 含义 |
|---|---|
| `kch` / `kch_id` / `kcmc` | 课程号 / 内部 ID / 课程名 |
| `xf` | 学分（数值或字符串视字段而定，注意） |
| `kcxzmc` / `kcxzdm` | 课程性质（"必修" / "限选" / "个性化课程"，代码 01/02/07） |
| `kclbmc` | 课程类别（"专业类教育课程" / "通识课" / "专业实践类课程" 等） |
| `kkbmmc` / `jgmc` | 开课部门 / 教研院系 |
| `zymc` / `zyh_id` / `njdm_id` | 培养计划归属专业 / 专业代码 / 年级 |
| `zyfxmc` / `zyfx_id` | 专业方向（默认 `wfx`/"无方向"） |
| `jyxdxnm` / `jyxdxqm` | 教育修读学年 / 学期（与申请时点） |
| `yyxdxnxqmc` | **应修读学年学期**字符串 `"YYYY-YYYY/N"`，多学期用逗号串（"2025-2026/2,2025-2026/3,2025-2026/1,..."）— CLI 端要 split |
| `xfyqjd_id` / `xfyqjdmc` | 学分要求节点 ID + 名（"必修" / "专业选修课" / "专业实践类课程"），可与 N551225 join |
| `xsxxxx` | 学时信息字符串，`"理论(1.0)" / "实验(3.0)" / "其他(0.5)"`，**含小数学分** |
| `zxs` | 总学时（数值） |
| `xqmc` | 校区 |
| `xdlx` | 修读类型（`zx`=主修） |

**冗余 / scaffolding**：`sqztmc / shzt / sfyx / sfcj / sfsjk / drhkc / fxbj / exwbj / ezybj / zyhxkcbj / zyzgkcbj / jcbj / jcbjmc / sflsmc / szxhxkc / zxbj / qsjsz / row_id / totalresult / xsdm_0X`(各类学时分布字段名变化) / `jxzxjhxx_id / jxzxjhkcxx_id`

**已知坑**
- response 是**全校跨专业**列表，CLI 必须按 `zyh_id` 过滤；从 N305005 / N551225 任意一发结果里取当前用户的 `zyh_id` (`050207` 等专业代码)
- `yyxdxnxqmc` 是**逗号串多值**而不是单值（同一门课在多学期都可修），CLI parse 时切 `,` 取所有
- `xsxxxx` 是含括号字符串，需 regex `(.+?)\((\d+(\.\d+)?)\)` 拆类型 + 学分
- `xsdm_01 / xsdm_02 / xsdm_07` 等可选字段名按 `kcxzdm` 变化（01 类用 `xsdm_01`，02 类用 `xsdm_02`），动态字段反序列化用 `Map<String, Value>` 兜底

**N153540（专业培养计划查询）** = 沿用同一 endpoint family（路径 `/jxzxjhgl/jxzxjhck_cxJxzxjhckIndex.html`）+ 同字段，但**默认仅当前用户专业**（不需要 `zyh_id` 过滤）。CLI 实装可二选一：N153521 + 客户端过滤 vs N153540 直接走，后者代码简单但 ZF 行为依赖；MVP 选 **N153521 + 显式 `zyh_id`** 更可控。

---

### 2.9 N532560 — 毕业设计成绩查看 ✅ 2026-04-26 真机抓（端点 + envelope，items 空因用户未到毕设阶段）

**端点**

```
POST /xsbysjgl/cjck_cxCjckIndex.html?doType=query&gnmkdm=N532560
```

**form 字段**：仅公共 `_search/nd/queryModel.*/time`，**无 xnm/xqm**（学年学期由服务端按"当前毕业设计学年学期"决定）。

**响应**：标准 §1.3 分页 envelope。空数据时 `items:[], totalResult:0` 友好返回（非 4xx）。

**页面文案规格**（grid header，确认 item 字段命名预期）：

```
学年 / 学期 / 题目名称 / 学号 / 姓名 / 学院 / 年级 / 专业
/ 百分制总评成绩 / 五级制总评成绩 / 合成状态 / 查看(详情链接)
```

**已知坑**
- 页面顶部显示"当前毕业设计学年学期:**2018-2019** 学年第 1 学期" —— ZF 此 SP 的"当前毕业设计学年"配置看起来很久没更新（2026 仍指 2018），CLI 不要从此 SP 反推"当前学年"
- 用户为 23 级本科未到毕设阶段，items 为空属预期；CLI 应在 `currentResult==0 && totalResult==0` 时返友好提示，**不当错误处理**
- items 字段名（`xnmmc/xqmmc/bymc(题目)/xh/xm/jgmc/njmc/zymc/bfzcj/wjzcj/hczt/查看链接`）按 §1.3 ZF 范式预期，毕业班用户实地抓后再敲实

---

---

---

## 3. SP 待验清单（按优先级）

| 优先 | gnmkdm | 名称 | 路径（拼到 i.sjtu.edu.cn 前缀） | 状态 |
|---|---|---|---|---|
| P0 | N305005 | 学生成绩查询 | /cjcx/cjcx_cxDgXscj.html | ✅ 已验 §2.1 |
| P1 | N2151   | 个人课表查询（学年学期） | /kbcx/xskbcx_cxXskbcxIndex.html | ✅ 已验 §2.2 |
| P1 | N309131 | GPA / 学积分查询 | /cjpmtj/gpapmtj_cxGpaxjfcxIndex.html | ✅ 已验 §2.3 |
| P1 | N358105 | 考试信息查询 | /kwgl/kscx_cxXsksxxIndex.html | ✅ 已验 §2.4 |
| P2 | N2154   | 学生课表查询（按周次） | /kbcx/xskbcxMobile_cxXsKb.html | ✅ 已验 §2.7 |
| P2 | N305007 | 学生成绩明细查询（含小项分） | /cjcx/cjcx_cxDgXsxmcj.html | ✅ 已验 §2.5 |
| P2 | N551225 | 学生修业情况查询 | /xjyj/xsxyqk_ckXsXyxxHtmlView.html | ✅ 已验 §2.6 |
| P3 | N153521 | 培养计划课程查询 | /jxzxjhgl/pyjhkcxxcx_cxPyjhkcxxIndex.html | ✅ 已验 §2.8 |
| P3 | N153540 | 专业培养计划查询 | /jxzxjhgl/jxzxjhck_cxJxzxjhckIndex.html | ✅ 归并 §2.8 末段 |
| P3 | N532560 | 毕业设计成绩查看 | /xsbysjgl/cjck_cxCjckIndex.html | ✅ 端点已验 §2.9（items 待毕业班补） |

**红线规避**：信息维护 N100802 / N100808 / N1532、选课 N253519 / N253512、教学评价 N401605 / N401650、报名申请 N\* 全集合 —— 一律不调研、不实现。

---

## 4. 已知 SP 字典端点（页面加载副作用）

`/xtgl/zdpz_cxZdpzList.html?gnmkdm=<gnmkdm>` —— ZF 字典配置 POST，页面加载时自动调；返回该 SP 的下拉框可选值。CLI 不需要单独调（form 字段值在 SP 各自调研里硬编码即可），仅记录一下避免被当成数据接口误抓。

---

## 5. 半自动调研 SOP（每个新 SP）

1. 我：`navigate_page` 到 `https://i.sjtu.edu.cn<path>?gnmkdm=<gnmkdm>&layout=default`
2. 我：`take_snapshot` + `evaluate_script` 抓 form / select / hidden / queryButtons
3. 我：报告"请点 [按钮名]"给用户
4. 用户：点查询 / 检索按钮
5. 用户回 "点了"，我：`list_network_requests` → `get_network_request` 抓 `doType=query` 那一发
6. 我：把 endpoint / form / response shape 追加到 §2，红线无关字段一律脱敏
7. 我：报告并请求下一个 SP 方向
