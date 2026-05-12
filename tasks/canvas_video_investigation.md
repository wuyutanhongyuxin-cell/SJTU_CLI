# Canvas 课堂视频 (v.sjtu.edu.cn / 交我学) 调研规格

> **本文件只记录字段定义、接口形态、鉴权链；不存任何真实学号 / 姓名 / cookie / token / 视频 URL 真值**。

调研日期：2026-05-07
调研范式：复用 i.sjtu / 办事大厅 / 电费 同款"半自动 SOP"——用户在 chrome 完成 LTI launch，CLI 端通过 chrome-devtools MCP 只读抓 network。
红线裁量：CLAUDE.md "i.sjtu / 交我办 硬红线" 不适用本系统（v.sjtu 是 SJTU 自家视频后台，无写按钮/选课/信息维护类副作用），但仍按"只读访客"原则——CLI 实装只调查询端点，不调埋点 (`burialPoint`) / 不写"上次观看时间" / 不上传视频笔记。

参考实现：[Okabe-Rintarou-0/SJTU-Canvas-Helper](https://github.com/Okabe-Rintarou-0/SJTU-Canvas-Helper)（MIT）。本调研**不 vendor**，仅参考思路重写。

---

## 1. 系统拓扑

```
oc.sjtu.edu.cn (Canvas LMS)
   │
   │  课程左栏 LTI tool id=8329 = "课堂视频new"
   │  course_navigation.url = https://v.sjtu.edu.cn/jy-application-canvas-sjtu/lti3/lti3Auth/ivs
   ▼
LTI 1.3 OIDC implicit flow (response_type=id_token + form_post)
   │
   ▼
v.sjtu.edu.cn/jy-application-canvas-sjtu/...    (后端 REST，本调研主目标)
v.sjtu.edu.cn/jy-application-canvas-sjtu-ui/... (前端 SPA，hash 路由)
   │
   │  实际 mp4 直链
   ▼
live.sjtu.edu.cn/vod/.../*.mp4?key=<时效签名>   (CDN，Range 分片下载)
```

子系统名约定（CLI 端 sub-session 隔离）：`canvas_video`（独立于现有 `canvas` PAT 路径，因为 PAT **走不通**这条链）。

---

## 2. LTI 1.3 鉴权链（CLI 实装最难点）

### 2.1 launch 起点

**入口 URL**（GET）：

```
https://oc.sjtu.edu.cn/courses/<canvas_course_id>/external_tools/8329?display=borderless
```

`canvas_course_id` 是 Canvas 内部数字 ID（例：`88168`）。可通过现有 sjtu-cli Canvas PAT 路径调 `/api/v1/courses?enrollment_state=active` 列举（已实装）。

LTI tool id `8329` 在所有课程下相同，可能未来更换。预防方案：CLI 调 `/api/v1/courses/<id>/tabs` 找 `label == "课堂视频new"` 的 tab 取动态 tool_id。

### 2.2 LTI 1.3 OIDC 三跳

| 跳 | 方向 | 形态 |
|---|---|---|
| 1 | GET oc.sjtu/courses/.../external_tools/8329 | 302 → oc.sjtu/api/lti/authorize?... |
| 2 | GET oc.sjtu/api/lti/authorize?... | 200 HTML 含 `<form action="https://v.sjtu.edu.cn/jy-application-canvas-sjtu/lti3/lti3Auth/ivs" method="post">` 自动 submit；hidden 字段 `id_token`、`state` |
| 3 | POST v.sjtu/jy-application-canvas-sjtu/lti3/lti3Auth/ivs (Content-Type: application/x-www-form-urlencoded) | 302 → v.sjtu/jy-application-canvas-sjtu-ui/#/ivsModules/index?tokenId=<X> |

### 2.3 OIDC authorize 端点参数（跳 2 的 query）

| 字段 | 含义 |
|---|---|
| `client_id` | LTI 1.3 client id（实测：`10000000000025`，全局常量） |
| `login_hint` | 用户标识 hint（40-hex） |
| `lti_message_hint` | JWT(HS256) 含 `verifier` / `canvas_domain` / `context_type` / `context_id` / `canvas_locale` |
| `nonce` | 64-hex 随机数（防重放） |
| `prompt` | `none` |
| `redirect_uri` | `https://v.sjtu.edu.cn/jy-application-canvas-sjtu/lti3/lti3Auth/ivs` |
| `response_mode` | `form_post` |
| `response_type` | `id_token` |
| `scope` | `openid` |
| `state` | JWT(RS256) 含 oc.sjtu issuer/subject/expiration |

### 2.4 拿 tokenId

跳 3 完成后，浏览器从 `Location` header 提 `tokenId` query 参数。**这是后续 API 调用的根 token**（约 600 字符，base64-url-safe 字符集）。

### 2.5 实装难点

CLI 要复刻这条链，需要：

1. 跟 302 follow（reqwest `Policy::none()` 手动循环，复用 S2 `cas::follow_redirect_chain` 范式）
2. 解析 HTML form_post：
   - 解析 `<form action="..." method="post">`
   - 提取 hidden `name=id_token value=...` + `name=state value=...`
   - 用 reqwest POST 自动 submit
3. 跟 302 拿到 `tokenId` query
4. **JAccount session 复用**：跳 1 的 GET 必须带 oc.sjtu 的 JAccount session cookie（CAS 已签发） → CLI 复用 `cas_login("canvas", "https://oc.sjtu.edu.cn/")` 拿 cookie
5. 跳 3 之后 v.sjtu 会下发 `JSESSIONID` + `route` cookie，必须保存到 sub_session

**简化方案（推荐）**：第一版用 `headless_chrome`（S1 已有依赖）跑完整 LTI launch + 等 SPA hash 出现 → 从 chrome 提取 tokenId + cookies。比手刻 OIDC 链更稳，复用浏览器自动处理 JS 跳转 + cookie。

---

## 3. 关键 API 端点速查

| 端点 | 方法 | Content-Type | 用途 |
|---|---|---|---|
| `/jy-application-canvas-sjtu/lti3/getAccessTokenByTokenId?tokenId=<X>` | GET | — | tokenId → token + courId 换取 |
| `/jy-application-canvas-sjtu/sjtu/teaching-class/getByLtiCourseId/<32-hex>` | POST (空 body) | — | 查询课程基本信息（视频列表前置调用，可不调） |
| `/jy-application-canvas-sjtu/directOnDemandPlay/findVodVideoList` | POST | `application/json` | 列视频（16 讲）|
| `/jy-application-canvas-sjtu/directOnDemandPlay/getVodVideoInfos` | POST | `multipart/form-data` 或 `application/x-www-form-urlencoded` | 拿单讲 mp4 URL |
| `/jy-application-canvas-sjtu/sjtu/video/watch/record/last?courseId=<32-hex>` | GET | — | 查上次观看（CLI 不调，只读但无业务用途）|

**通用必带 headers**（除 1.LTI launch 之外）：

```
referer: https://v.sjtu.edu.cn/jy-application-canvas-sjtu-ui/
token: <data.token from getAccessTokenByTokenId>
accept: application/json, text/plain, */*
content-type: application/json   # findVodVideoList
content-type: application/x-www-form-urlencoded   # getVodVideoInfos
cookie: JSESSIONID=...; route=...
user-agent: <真实 UA>
```

**通用响应包装**：

```json
{ "code": "0", "data": { /* ... */ }, "message": null, "status": 200, "success": true, "timestamp": <unix-ms> }
```

`code=="0" && success==true` 视作业务成功。

---

## 4. `getAccessTokenByTokenId` 详细规格

**端点**：`GET /jy-application-canvas-sjtu/lti3/getAccessTokenByTokenId?tokenId=<X>`

**鉴权**：跳 3 完成后 v.sjtu 的 `JSESSIONID` + `route` cookie；**不需要** token header（这是首次换 token 的入口）。

**响应**（关键字段）：

```json
{
  "code": "0",
  "data": {
    "accessToken": {
      "jwt_token": "eyJhbGciOiJSUzI1NiJ9.<base64url>...",   // RS256 JWT，含 sub=<userCode> PII（CLI 不解码不持久化）
      "access_token": null, "clientSign": null, "expires_in": null, "first": null,
      "refresh_token": null, "scope": null, "token_type": null, "userIp": null, "username": null
    },
    "ivsUserId": "<内部数字 ID>",
    "params": {
      "courseName": "<课程全名 含工号格式（PII 不入日志）>",
      "clientId": "10000000000025",
      "courId": "<base64+/= 字符的加密 token>",                // ★ findVodVideoList 的 canvasCourseId（用 URL-encoding）
      "ltiCourseId": "<32-hex>"                                  // 内部 LTI 课程 ID（teclCode/teachingClassId 同值）
    },
    "roles": ["StudentEnrollment"],
    "tenantOrgCode": null, "space": null, "authSpace": null,
    "token": "eyJhbGciOiJIUzUxMiIsInppcCI6IkdaSVAifQ.<base64url>...",   // ★ 后续 API token header 用这个
    "userCode": "<真工号 PII 不入日志>",
    "userName": "<真姓名 PII 不入日志>"
  },
  "message": null, "status": 200, "success": true, "timestamp": <unix-ms>
}
```

**CLI 提取逻辑**：

```rust
struct Bootstrap {
    token: String,         // data.token
    cour_id: String,       // data.params.courId（保留原文，POST body 时再 URL-encode）
    lti_course_id: String, // data.params.ltiCourseId
}
// data.userCode / userName / accessToken.jwt_token 含 PII，**不持久化、不打日志**
```

---

## 5. `findVodVideoList` 详细规格

**端点**：`POST /jy-application-canvas-sjtu/directOnDemandPlay/findVodVideoList`

**Headers**：`token: <data.token>` + 通用 headers + `content-type: application/json`

**请求 body**：

```json
{ "canvasCourseId": "<URL-encoded data.params.courId>" }
```

注意：服务端**显式期望 URL-encoded 后再放进 JSON**（`/` → `%2F`，`+` → `%2B`，`=` 不变）。SJTU-Canvas-Helper 用 `urlencoding::encode(...)` 处理；CLI 端用 `percent-encoding` crate 走 `NON_ALPHANUMERIC` 集（与浏览器 `encodeURIComponent` 一致）。

**响应** `data.records[]`（核心字段）：

| 字段 | 类型 | 含义 | CLI 暴露策略 |
|---|---|---|---|
| `videoId` | string (base64=) | **每讲的视频 ID**，`getVodVideoInfos` 入参 | ★ 必留 |
| `videoName` | string | 形如 `<课程名>(第N讲)` | ★ 必留 |
| `courseBeginTime` | string `YYYY-MM-DD HH:MM:SS` | 上课开始 | 必留 |
| `courseEndTime` | string `YYYY-MM-DD HH:MM:SS` | 上课结束 | 必留 |
| `classroomName` | string | 教室名（如 `上院210`）| 留 |
| `userName` | string | **教师真姓名**（不是学生）—— PII 但教师姓名公开属性可留 | 默认输出，`--redact-teacher` 抹掉 |
| `courId` | int | 课次内部 ID | 留作 join key |
| `videAuditStatus` | int | 视频审核状态（3=已审通过等）| 留作过滤 |
| `playCount`, `playTime`, `playTimes`, `playAverage` | num | 个人观看统计 | **抹掉**（含个人观看习惯，PII 边界）|
| `partClose`, `uebung`, `subjId`, `subjImgUrl`, `videImgUrl`, `videSource`, `inSchoolVodStatus`, `csplImgUrl`, `courVideoEditStatus`, `teclId` | mixed | 系统字段 | 丢 |

**响应分页**：`data` 里有 `total: 16, size: 2000, current: 1, pages: 1, records: [...]`。MVP 不分页（`size=2000` 单页够用，一门课一学期最多 ~50 讲）。

---

## 6. `getVodVideoInfos` 详细规格

**端点**：`POST /jy-application-canvas-sjtu/directOnDemandPlay/getVodVideoInfos`

**Headers**：`token: <data.token>` + 通用 headers + `content-type: multipart/form-data` 或 `application/x-www-form-urlencoded`（SPA 实测用 multipart，但服务端应该兼容 urlencoded —— CLI 端用 urlencoded 更省字节）

**请求 form**：

```
playTypeHls=true
isAudit=true
id=<videoId from list, 注意原文带 = 后缀，需 URL-encode>
```

`isAudit=true` 语义未完全确认，疑似"是否带审计/审核标记"。CLI 跟 SPA 默认值，不改。

**响应** `data`（核心字段）：

| 字段 | 类型 | 含义 | CLI 暴露 |
|---|---|---|---|
| `id` | int | 视频内部 ID（`videVodId` 同义但不同号）| 内部用 |
| `videPlayTime` | int | 时长（秒）| ★ 必留 |
| `rtmpUrlHdv` | string URL | **HD mp4 直链（默认机位）** | ★ 必留 |
| `rtmpUrlFluency` / `rtmpUrlDistinct` / `rtmpUrlDefault` | string\|null | 流畅 / 标清 / 默认 多档（实测均 null，仅 HD 可用）| 留作 fallback |
| `videoPlayResponseVoList[]` | array | **多机位**（`cdviChannelNum=0` 老师 / `=1` PPT），每元素含独立 `rtmpUrlHdv` | ★ 必留 |
| `videSrtUrl` | string\|null | 字幕 URL | 留 |
| `vodurl` | string URL | `courses.sjtu.edu.cn` 的 redirect 链路（CLI 不调，老 LTI 旧版用）| 丢 |
| `videName` / `subjName` / `courName` | string | 视频名 / 课程名 | 留 |
| `videBeginTime` / `videEndTime` / `videBeginTimeMs` / `videEndTimeMs` | mixed | 视频实际录制时间 | 留 |
| `lastWatchTime` | int (秒) | 个人上次观看进度（PII 边界）| **抹掉** |
| `userCode` (`08452`) / `userName` (`<教师姓名>`) / `teclName` / `teclCode` (32-hex) / `teclId` | mixed | 教师 + 教学班标识 | 教师姓名留，`userCode` 抹掉 |
| `organizationCode` (`14000`) / `organizationName` | string | 学院代码 + 名 | 留学院名 |
| `subjCode` (`FL3426`) | string | 课程代码 | 留 |
| `clroName` (`上院210`) / `clroId` | mixed | 教室 | 留 |
| `videPlayCount`, `videCommentCount`, `videCommentAverage`, `videRecordChannelNum` | num | 全平台统计 | 丢 |
| `invalidCourse`, `partClose`, `uebung`, `clipIdentifier`, `cminId`, `smseId`, `loginUserId`, `videVodId`, `userId`, `userAvatar`, `deviPuid` | mixed | 系统字段 | 丢 |

---

## 7. mp4 URL 形态 + 下载

**URL 模板**：

```
https://live.sjtu.edu.cn/vod/<11-digit>/<11-digit>/<channel>_<sortKeyA>-<sortKeyB>.mp4?key=<unix>-<n>-<md5>
```

例：

```
https://live.sjtu.edu.cn/vod/<orgId>/<deviceId>/0_<startMs>-<endMs>.mp4?key=<unix>-1-<md5>
                                                  ^                          ^
                                            channel=0/1 (机位)         时效签名（unix 秒，疑似几小时失效）
```

- `key` query 是**预签名**，含 unix 秒时间戳 + md5 形式校验码 → **mp4 URL 不可长期缓存，每次下载前重新调 getVodVideoInfos 拿新 URL**
- 实测无 `Range` 限制：服务端返回 `Accept-Ranges: bytes`，标准 HTTP `Range: bytes=A-B` 分片下载支持
- **无 DRM / 无 AES**：mp4 是裸 H.264 + AAC，可直接 ffmpeg `-c copy` 抽流

**下载策略（MVP）**：

```rust
// 单文件并发：把 mp4 切成 N 段（N=8），每段 reqwest stream + Range header，写到 tmp 然后 cat
// 失败重试：单段失败 3 次后整体重发 getVodVideoInfos 拿新 URL（防止 key 过期）
// 必带 header: Referer: https://courses.sjtu.edu.cn  （SJTU-Canvas-Helper 实测必需）
```

**音频提取**（`--audio-only`）：

```
ffmpeg -i input.mp4 -vn -acodec copy -y output.m4a
```

不转码，纯 mux 抽 AAC 流到 m4a 容器。`ffmpeg` 走 subprocess 调用，sjtu-cli 不打包 binary —— 文档说明用户需 `winget install ffmpeg` 或类似。CLI 启动时检测 `ffmpeg --version` 不到则在 `--audio-only` 路径报友好错。

---

## 8. 已知坑 / PII / 实装建议

1. **PII 满天飞**：`getAccessTokenByTokenId` 响应里 `userCode`（真工号）、`userName`（真姓名）、`accessToken.jwt_token`（sub 含工号）；`getVodVideoInfos` 里 `userCode`（教师工号）/`userName`（教师姓名）/`organizationCode`（学院码）/`courseName`（课程全名）。**CLI 实装严守**：
   - 落盘 sub-session 仅持久化 `data.token` + cookies，**不持久化** userCode/userName/jwt_token
   - 默认输出 envelope 里学生端 PII 字段全抹（`userCode` 默认 None），教师姓名留（教学公开属性），`--with-identity` 才出全部
   - 日志（tracing）只打字段名 + 长度，绝不打 token / userCode 真值

2. **token 来源混淆**：响应里有两个 token —— `data.accessToken.jwt_token`（RS256，应该是 LTI 标准的 access_token）和 `data.token`（HS512+GZIP）。**实测 SPA 后续所有请求 header `token` 用的是 `data.token`，不是 `jwt_token`**。CLI 端只取 `data.token`。

3. **canvasCourseId 不是数字 ID**：是一段 RSA/AES 加密的 base64+/= 字符串，从 `data.params.courId` 取。**直接用 Canvas 数字 id `88168` 会 500 "登录信息无效"**（已实测）。

4. **token 时效未测**：HS512 token 多久过期未现场抓多次。SJTU-Canvas-Helper 提到有 `refresh_token` 字段（响应中 null），猜测是 1-12 小时短期。CLI 端**不缓存**，每次完整 LTI launch 重新拿（成本 ~3-10s）。

5. **mp4 URL 时效**：`key` 含 unix 秒签名，疑似 1-3 小时失效。**CLI 端：批量下载 16 讲时不要预先 list 所有 URL 再统一下，必须**边调 getVodVideoInfos 边下载，单讲下完才拿下一讲的 URL。

6. **多机位**：`videoPlayResponseVoList[].rtmpUrlHdv` 通常 2 路（老师正面 / PPT）。MVP 默认下 `cdviChannelNum=0` 那一路（与 SPA 默认一致）；`--all-channels` 可全下。

7. **Kaspersky 杀软干扰**：实测 chrome-devtools MCP 跑 v.sjtu SPA 时有 Kaspersky `gc.kis.v2.scr.kaspersky-labs.com` 注入 `x-kl-saas-ajax-request: Ajax_Request` header，**这个 header CLI 端不需要**（仅本机杀软插桩）。

8. **不调埋点**：SPA 启动时调 `classroomRecordStatistics/burialPoint`（POST，写埋点）—— **CLI 实装不调**，仅查询。

9. **`videAuditStatus` 过滤**：list 响应里 `videAuditStatus=3` 表示"已审核通过/可观看"。CLI 默认 filter `==3`，其他状态报 "尚未审核" 友好错。

---

## 9. CLI 命令设计（实装预案）

接入位置：现有 `apps/canvas/` 模块（与 PAT 路径并列，但独立 sub-session 文件 `canvas_video.json`）。或独立 `apps/canvas_video/` 子模块（推荐，避免与 PAT 路径混淆）。

```
sjtu canvas videos list <course_id>           # 列 16 讲
sjtu canvas videos list <course_id> --json    # JSON envelope 输出
sjtu canvas videos download <course_id>       # 全下到 ./<course_name>/
  [--lecture <n>]            # 仅下第 n 讲
  [--lectures <range>]       # 例 1-5,8,12-16
  [--to <dir>]               # 输出目录
  [--audio-only]             # 仅抽 m4a（需 ffmpeg）
  [--all-channels]           # 双机位都下（默认仅老师视角）
  [--concurrency 8]          # 单文件分片并发数
  [--with-identity]          # 输出含 PII 字段（教师工号 / 学生 lastWatchTime 等）
```

文件命名：`<course_name>_第<NN>讲_<开课日期>.mp4`，如 `日语语言学专题研讨2_第01讲_2026-03-06.mp4`（避空格用下划线 / 数字补 0）。

---

## 10. CP 列表（CP-V0..CP-V4）

- [x] **CP-V0** 调研（本文件）— 2026-05-07 ✅
- [x] **CP-V0.1** 用户口头确认契约 + 实装路线（headless_chrome vs 手刻 OIDC）✅
- [x] **CP-V1** 实装 LTI launch + token 提取 → `apps/canvas_video/auth.rs`（或 `cas_lti.rs`）✅
- [x] **CP-V2** 实装 list + 字段脱敏 → `sjtu canvas-video list <id>`（真机 9 讲全列出）✅
- [x] **CP-V3** 实装单讲下载 + Range 分片并发 → `sjtu canvas-video download <id> --lecture 1`（单讲落盘 + 可播放）✅
- [x] **CP-V4** 实装批量 + ffmpeg 音频提取 → `sjtu canvas-video download <id> --lectures 1-9 --audio-only`（9 讲 m4a 全部落盘）✅
- [x] mockito 端单测（不打真服务器）—— 91 unit tests pass ✅

---

## 11. V5 性能优化系列（V5.A → V5.F 2026-05-09..2026-05-12）

CP-V3/V4 实装后启动 audio-only 加速路线，3 轮试错 + 1 轮收尾，最终撤回到 V5.A baseline。

| 阶段 | 路径 | 单讲 / 9 讲 batch | 结果 |
|---|---|---|---|
| V5.A baseline | mp4-full + ffmpeg | ~2 min / ~18 min | ✅ |
| V5.B chunk-Range | H1.1 × 8 chunk-level | 20.7 min / —（body 流挂）| ❌ |
| V5.D sample-Range merge | gap=64KB + mp4 box parse | 6.5 min / ~60 min（705 MB）| ⚠ 偏离目标 |
| V5.E-B+ H2 池 + Dynamic P85 | 4-Client + range 哈希分桶 | 30.5 min / > 4h | ❌ **反向退化 4.7×** |
| **V5.F 撤回**（current）| 删 audio-only 整路，回 V5.A | **1.74 min / 15.13 min** | ✅ ≤ 25 min 余量 40% |

**核心结论**：SJTU CDN 单 H1.1 sustain throughput 11.4 MB/s（fat-pipe 限速）+ moov-end mp4 + 无 audio-only endpoint，audio-only Range 优化的理论上限远高于"整下并发"。

**完整复盘 + 知识沉淀**：`docs/superpowers/research/2026-05-12-v5-series-retrospective.md`（含三轮 web research 引用 / mp4 box / HTTP/2 multiplexing / Tengine 配置 / 已排除方案矩阵 / 复用规则）

**lessons**：`tasks/lessons.md` 2026-05-11 段（V5.D 工程妥协）+ 2026-05-12 段（V5.F 5 条规则）

---

> 本调研全程只读：仅 GET 鉴权链 + GET token + POST 查询 list/info；未触发任何"提交 / 修改 / 上传 / 评论 / 笔记 / 埋点"等写副作用。
