# SJTU-CLI Agent 使用指南

> 本文件面向 AI Agent（如 Claude）调用 SJTU-CLI 时的参考手册。
> 默认命令输出走标准 Envelope（`ok` / `data` / `error`），管道时自动 YAML，指定 `--json` 输出 JSON；**例外**：`sjtu jwc calendar` 在不带 `--json` / `--yaml` 且不带 `--to` 时直接输出原始 `.ics`。

---

## 通用约定

- **Envelope schema**：`{ ok: bool, data: T | null, error: { code, message } | null, schema_version: "1" }`
- **exit code**：0 = ok（含 fail-soft 的命令）；非 0 = 硬错误（鉴权失败 / 参数错误等）
- **日志脱敏**：学号/姓名/cookie 默认不进 stdout；`--with-identity` 才全出（部分命令）
- **管道友好**：默认 stdout 仅 envelope；`sjtu jwc calendar` raw 模式例外，stdout 直接是 `.ics`；stderr 仅进度/警告行，不影响 jq/yq 解析

---

## 认证命令

### sjtu login
JAccount 扫码登录，headless_chrome 弹浏览器窗口。

```bash
sjtu login                  # 弹浏览器，扫码，cookie 落 ~/.sjtu-cli/session.json
sjtu login --browser rookie # 备用：从本机已登录 Chrome/Edge 提取 cookie
sjtu status                 # 查看 session 是否有效
sjtu logout                 # 清除主 session（建议同时手动删 sub_sessions/）
```

---

## 教务命令（jwc）

> 后端：`i.sjtu.edu.cn`（ZF 正方 9 SP），CAS 认证，sub_session 缓存到 `sub_sessions/jwc.json`。

### sjtu jwc grades
单学期成绩查询（N305005）。

**入参**：
- `--xnm <YYYY>`（默认按今天日期推当前学年）
- `--xqm <3|12|16>`（3=秋 / 12=春 / 16=夏，默认推当前学期）
- `--page / --limit`

**输出关键字段**：`data.items[]`，每条含 `cj`（总评）/ `bfzcj`（百分制）/ `jd`（绩点）/ `xfjd`（学分绩点）。

### sjtu jwc gpa
单学期 GPA + 排名查询（N309131 两阶段）。

**入参**：
- `--xnm / --xqm`（同 grades）
- `--scope hxkc|qbkc`（核心课 / 全部课，默认 hxkc）
- `--rank njzy|nj|bj`（年级专业 / 年级 / 班级，默认 njzy）

**输出关键字段**：
- `data.gpa`：当前范围 GPA 字符串
- `data.gpapm`：排名原字符串（如 `"3/120"`）
- `data.gpapmParsed`：解析结构 `{ rank, total, percentile }`（`null` 若解析失败）
- `data.xjf`：学积分
- `data.xjfpmParsed`：学积分排名解析结构

**注意**：`rank=nj`（纯年级）在 SJTU 部分实例上可能 server 返 HTML 错误页 → exit 1；建议优先 `njzy`。

### sjtu jwc gpa-by-semester
多学期 GPA 对比，客户端循环 N309131。

**入参**：
- `--scope hxkc|qbkc`（默认 hxkc）
- `--rank njzy|nj|bj`（默认 njzy）
- `--xnm-from <YYYY>`（默认当年 - 3，覆盖 4 年制本科；非 4 年学制需手给）
- `--xnm-to <YYYY>`（默认当年）

**输出（关键字段）**：
- `data.requested[]`：所有 (xnm, xqm) 组合（含落空的）
- `data.succeeded[]`：成功学期，每条含 `gpapmParsed.rank/total/percentile` 解析结构
- `data.failed[]`：失败学期，`reason` 区分 "items 空" / 网络挂 / step1 拒绝
- exit code **始终 0**；agent 通过 `failed.len()` 判断是否需要重试

**真机典型耗时**：12 学期 ~50-60 秒（N309131 step1 server-side 统计每次 4-5s + 600ms throttle × 12）。

**注意**：
- `rank=nj`（纯年级）在 SJTU 部分实例上可能 server 返 HTML 错误页 → fail-soft 装进 `failed[]`，agent 建议优先 `njzy`
- 同 `xnxq` 传给 `qs_xnxq` + `zz_xnxq` 时 server 给的是 "截至该学期的累计 GPA"，不是该学期独自

### sjtu jwc schedule
整学期课表（N2151）。

**入参**：`--xnm / --xqm`

### sjtu jwc today / week / next
基于 N2154 周次端点的课表衍生命令。

```bash
sjtu jwc today --grid                        # 今日剩余课，comfy-table 渲染
sjtu jwc week --zs 14 --grid                 # 第 14 周整周课表
sjtu jwc next --within 7 --limit 10 --yaml   # 未来 7 天前 10 节课
```

**真实约束**：ZF 9 SP 不接受空 `xnm`/`xqm`；CLI 按当前日期推默认（春/秋/夏），`--xnm`/`--xqm` 可覆盖。

### sjtu jwc calendar
课表 + 考试 + 学年校历导出为 RFC 5545 `.ics`。

**入参**：
- `--xnm / --xqm`（同 grades；留空按今天自动推断）
- `--to <PATH>`（把 `.ics` 写到文件；**带 `--to` 也触发 envelope 模式** —— stdout 改输出 envelope，raw `.ics` 只去文件）
- `--no-academic`（跳过校历整天事件）
- `--no-exams`（跳过考试事件）
- `--json / --yaml`（强制输出 envelope；raw `.ics` 不再直写 stdout）

**典型用法**：
```bash
sjtu jwc calendar > schedule.ics
sjtu jwc calendar --xnm 2025 --xqm 12 --to ~/Desktop/sjtu.ics
sjtu jwc calendar --to /tmp/sjtu.ics --json
```

### `sjtu jwc calendar --json` / `--yaml` envelope schema

```json
{
  "ok": true,
  "schema_version": "1",
  "data": {
    "xnm": "2025",
    "xqm": "12",
    "eventCount": 87,
    "byKind": {
      "class": 64,
      "exam": 9,
      "academic": 14
    },
    "hashHex": "85944171f73967e8",
    "bytes": 24576,
    "warnings": []
  },
  "error": null
}
```

- `data.eventCount`：最终写入 `.ics` 的 VEVENT 总数
- `data.byKind`：按 `class` / `exam` / `academic` 三类计数
- `data.hashHex`：整份 `.ics` 的 FNV-1a 64-bit hash hex；**仅供单次调用内 sanity check**（如对比是否被外部修改）。跨次调用 hashHex 会因 `DTSTAMP` 字段刷新而不同，**真正的幂等保证靠 VEVENT 的 UID**（FNV-1a 基于学年/学期/类型/课号确定性生成），客户端按 UID 去重
- `data.bytes`：`.ics` 字节数
- `data.warnings[]`：fail-soft 警告；例如课表 / 考试 / 学年校历三路里某一路失败时，这里会保留原因字符串

**注意**：agent 若既要结构化摘要又要实际文件，推荐显式带 `--to <path> --json`。

### sjtu jwc exams
考试信息查询（N358105）。

---

## 水源命令（shuiyuan）

> 后端：`shuiyuan.sjtu.edu.cn`（Discourse），OAuth2 认证。

```bash
sjtu shuiyuan latest --limit 10 --yaml
sjtu shuiyuan topic <ID> --post-limit 20
sjtu shuiyuan inbox --unread-only
sjtu shuiyuan search "<query>" --in post
sjtu shuiyuan messages --filter inbox
sjtu shuiyuan message <ID>
```

写操作（需 `--confirm`）：`reply / like / new-topic / delete-* / pm-send / archive-pm`

---

## 交我办消息（messages）

```bash
sjtu messages list --unread-only
sjtu messages show <ID>
sjtu messages read-all
```

---

## Canvas 作业（canvas）

> Personal Access Token 鉴权，`sub_sessions/canvas_token.txt`。

```bash
sjtu canvas setup                            # 粘贴 PAT
sjtu canvas today                            # 今日 DDL
sjtu canvas upcoming --days 14 --yaml        # 未来 14 天 DDL
```

---

## Canvas 视频（canvas-video）

> LTI 1.3 + JAccount，bootstrap 缓存 30min，首次 ~40s，命中缓存 ~2s。

```bash
sjtu canvas-video list <COURSE_ID> --tool-id <TOOL_ID>
sjtu canvas-video download <COURSE_ID> --tool-id <TOOL_ID> --lectures 1-9 --audio-only --to ./out
sjtu canvas-video clear-cache --all
```

单讲 ≤ 2.5 min / 9 讲 batch ≤ 16 min（V5.F 真机 baseline）。

---

## 办事大厅（services）


```bash
sjtu services pending --yaml
```

---

## 电费（elec）

> 金额全部 `rust_decimal::Decimal`，JSON 序列化为字符串，避开 f64 精度坑。

```bash
sjtu elec balance
sjtu elec usage
sjtu elec history --days 7
```

---

## 图书馆借阅（library）

> 后端：`weijieyue.lib.sjtu.edu.cn:8080`（HTTP 8080 plain text），jaccount OAuth dance，主 session 直透传。

```bash
# 当前借阅
sjtu library loans

# 历史借阅
sjtu library history

# 罚款（仅显示，不缴费）
sjtu library fines
```

均 `--yaml` / `--json` 可切换输出格式，meta.via 永远是 `weijieyue`。

---

## 邮箱命令（mail）

> 后端：`mail.sjtu.edu.cn`（Zimbra 8.x），JAccount SSO 跟链 + ZM_AUTH_TOKEN，主 jaccount session 直透传。

```bash
# 收件箱预览（默认 50 条）
sjtu mail list

# 仅未读
sjtu mail list --unread

# 关键字搜索（与 --unread 互斥）
sjtu mail list --search "奖学金"

# 分页（与 --limit 组合）
sjtu mail list --limit 20 --offset 20

# 读单封正文（不标已读）
sjtu mail read <ID>

# 别名
sjtu mail ls
```

均 `--yaml` / `--json` 可切换输出格式。

**输出关键字段（list）**：
- `data.query`：实际发到 Zimbra 的 search query（debug 回显）
- `data.count`：本次返回邮件数
- `data.offset`：本次分页偏移
- `data.has_more`：是否还有更多（翻页提示）
- `data.items[]`：邮件列表，每条含 `id` / `subject` / `from_display` / `from_address` / `date_ms` / `unread` / `size_bytes`（可选）

**输出关键字段（read）**：
- `data.id` / `data.subject` / `data.from_*` / `data.date_ms` / `data.unread`
- `data.to_addresses[]` / `data.cc_addresses[]`：地址列表，每条含 `address` / `display_name`
- `data.body_plain`：text/plain 正文；HTML-only 邮件为 null
- `data.body_warning`：HTML-only 等情况的提示文本

### 永久不实装的写端点（CLI 红线）

- `mail read` 编译期注入 `read="0" html="0" max="50000"`，**绝不**标记邮件已读
- 编译期不实装：SendMsgRequest（发送）/ SaveDraftRequest（草稿）/ 所有 `*Action`（标记 / 移动 / 删除）

---

## 一卡通命令（card）

> 后端：`api.sjtu.edu.cn/v1/me/card*`，OAuth2 Authorization Code，token 落 `~/.sjtu-cli/sub_sessions/card_oauth.json`。
> 与 jwc/elec/services（cookie + CAS）独立鉴权链。

### sjtu card auth

首次授权：弹浏览器同意 → 拿 access_token + refresh_token 落盘。

**入参**：
- `--client-id <ID>`（必填，从 `developer.sjtu.edu.cn` 申请）

**前置**：`~/.sjtu-cli/card_oauth_secret.txt`（含 client_secret，chmod 600）

**输出关键字段**：`data.ok` / `data.card_no_redacted`（如 `0012***`）/ `data.expires_in_secs` / `data.scope`

### sjtu card balance

当前卡余额查询。

**入参**：
- `--with-identity`（可选，默认 false）：出学号 / 姓名 / 单位 / 银行卡（前 4 + **** + 后 4）

**输出关键字段**：
- `data.card_no_redacted`：卡号脱敏（前 4 + `***`）
- `data.balance`：可用余额（Decimal 字符串如 `"284.25"`）
- `data.trans_balance`：可消费余额（同上）
- `data.lost` / `data.frozen`：bool
- `data.expire_date`：到期日（如 `"2026-08-31"`，可空）
- `data.face_type`：身份类型（如 `"学生"`）
- `data.user` / `data.bank_no_redacted` / `data.face_sub_type`：仅 `--with-identity` 时输出，否则字段 skip
- `data.from_cache`：始终 false（一卡通无 cache 路径）
- `data.elapsed_ms`：耗时

**注意**：`cardId`（物理卡 ID）**永远不出**，即便 `--with-identity`（防卡号克隆攻击面，spec §8 红线）。

### sjtu card history

时间窗口内消费记录。

**入参**：
- `--days N`：天数，默认 30，**范围 1..=365**（超出 exit 1，error.code=`invalid_input`）
- `--limit M`：单次最多返回多少条，默认 50；服务端硬限 100，CLI 自动 clamp

**前置**：先跑过 `sjtu card balance` 一次（用于把主卡号写入 session），否则 history 报错引导先 balance

**输出关键字段**：
- `data.card_no_redacted` / `data.begin_date_local` / `data.end_date_local`
- `data.returned`：本次返回条数
- `data.total`：服务端 total（可能 > returned）
- `data.transactions[]`：每条含 `consumed_at` (ISO 8601 +08:00) / `system` / `merchant_no` / `merchant` / `description` / `amount`（Decimal 字符串，消费为负） / `balance_after`
- `data.total_amount`：sum(amount)（Decimal 链式累加）
- `data.from_cache` / `data.elapsed_ms`

### Token refresh 透明行为

- access_token TTL ≈ 30 分钟；CLI 启动时预检 stale，主动 refresh
- API 调用过程中 errno=10002 / "Authentication Failed" / 401 → 触发 with_token_refresh 自动续期 + 重试 1 次
- 用户无需重新跑 `card auth`，除非 refresh_token 也失效（罕见）

### Envelope `meta` 字段（v1+，多路径子系统）

仅多路径子系统（当前：`card` 双轨 OAuth2/weixin）填 `meta`：

```yaml
meta:
  via: "oauth2" | "weixin"
  source_hint: "api.sjtu.edu.cn" | "weixin.sjtu.edu.cn"
```

Agent 用法：
- `meta.via=oauth2`：可有 `data.user / data.bank_no_redacted`（视 `--with-identity` flag）
- `meta.via=weixin`：上述 PII 字段**永远 None**（红线）；`data.card_no_redacted` 是占位字符串 `<weixin>`，**非个体卡号识别**
- 其他子系统（elec/shuiyuan/canvas/jwc/services）envelope **不含** `meta` 字段（保后向兼容）

### 永久不实装的写端点（CLI 红线）

`PUT /v1/me/card` 开卡 / `POST /v1/me/card` 挂失/解挂/改密码 / 充值 / 改照片 / 拾卡 — 一律不实装。
