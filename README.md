# SJTU-CLI

上海交通大学 JAccount 命令行工具。扫码登录一次，终端里直接查水源 / 交我办消息 / Canvas 作业 DDL，输出支持 YAML / JSON，方便 AI Agent 调用。

> **状态**：alpha — 已实装 9 大子系统（水源 / 交我办消息 / Canvas / 办事大厅 / 电费 / 教务 / 一卡通 / 图书馆 / 邮箱）；命令面稳定，仍在持续接入新子系统。仅供个人合规使用。

## 合规声明

- 只做读操作（写操作默认 `--confirm` 二次确认）
- 不做抢课 / 代登录 / 批量爬他人数据
- 本工具仅本机运行，session / token 落盘在 `~/.sjtu-cli/`（Unix 权限 600）
- 请遵守上海交通大学相关服务条款

## 已实装

| 命令 | 说明 |
|---|---|
| `sjtu login` | JAccount 扫码登录，cookie 落盘 `~/.sjtu-cli/session.json` |
| `sjtu status` / `logout` | session 状态查询 / 清除 |
| `sjtu shuiyuan latest\|topic\|inbox\|search\|messages\|message` | 水源社区（shuiyuan.sjtu.edu.cn）只读 |
| `sjtu shuiyuan reply\|like\|new-topic\|delete-*\|pm-send\|archive-pm` | 水源写操作（默认 `--confirm`） |
| `sjtu messages list\|show\|read-all` | 交我办消息中心（my.sjtu.edu.cn） |
| `sjtu canvas setup\|whoami\|today\|upcoming` | Canvas LMS（oc.sjtu.edu.cn）作业 DDL |
| `sjtu canvas-video list\|download\|clear-cache` | Canvas 课堂视频（v.sjtu.edu.cn）LTI 1.3 鉴权 + 单讲 / 批量 mp4 / `--audio-only` 抽 m4a |
| `sjtu services pending` | 办事大厅（my.sjtu.edu.cn）待办 / 已办 / 抄送 |
| `sjtu elec balance\|usage\|history` | 宿舍电费（elec.sjtu.edu.cn）—— 金额 `rust_decimal::Decimal` 硬约束 |
| `sjtu library loans\|history\|fines` | 图书馆借阅（weijieyue.lib.sjtu.edu.cn:8080）—— **只读**，永不实装续借 / 缴费 |
| `sjtu jwc grades\|schedule\|gpa\|exams\|today\|week\|next\|calendar` | 教务（i.sjtu.edu.cn）—— N305005 成绩 / N2151 学年学期课表 / N309131 GPA + 排名双轨 (`gpapmParsed` / `xjfpmParsed`) / N358105 考试 / N2154 衍生（今日 / 整周 / 接下来 N 天） / RFC 5545 iCal 导出（课表 + 考试 + 校历，FNV-1a UID 幂等）；`--grid` comfy-table 表格输出 |
| `sjtu jwc gpa-by-semester` | 多学期 GPA 对比（默认 4 年 × 3 学期 N309131 循环，600ms throttle，fail-soft：失败学期落 `failed[]`，exit 始终 0；真机 12 学期 ~56s） |
| `sjtu card auth\|balance\|history` | 一卡通（api.sjtu.edu.cn）—— OAuth2 Authorization Code，余额 + 消费记录只读；金额 `rust_decimal::Decimal` 硬约束；身份字段默认抹掉，`--with-identity` 才出 |
| `sjtu mail list [--unread] [--search <q>]` | 邮箱（mail.sjtu.edu.cn Zimbra）—— inbox 预览，`--unread` 仅未读，`--search` 关键字；别名 `mail ls` |
| `sjtu mail read <id>` | 邮箱单封正文（**不**标已读，编译期注入 `read="0"`）|

路线图 / 未完工事项见 `tasks/todo.md`。性能复盘 / 知识沉淀见 `docs/superpowers/research/`。

## 快速开始

```bash
git clone https://github.com/wuyutanhongyuxin-cell/SJTU_CLI.git
cd SJTU_CLI
cargo build --release
./target/release/sjtu --help
```

首次使用：

```bash
sjtu login                                  # 弹出浏览器，扫码登录 JAccount
sjtu shuiyuan latest --limit 5 --yaml       # 看水源最新 5 条
sjtu messages list --unread-only            # 交我办未读消息
```

Canvas 走 Personal Access Token（避免 SSO 折腾）：

```bash
# 浏览器打开 https://oc.sjtu.edu.cn/profile/settings
# → "+ 创建新访问许可证" → 复制 Token
sjtu canvas setup                           # 粘贴 Token
sjtu canvas upcoming --days 14 --yaml       # 未来 14 天作业 DDL
```

Canvas 课堂视频（v.sjtu.edu.cn）走 LTI 1.3 + JAccount 复用：

```bash
sjtu canvas-video list <COURSE_ID> --tool-id <TOOL_ID> --yaml
sjtu canvas-video download <COURSE_ID> --tool-id <TOOL_ID> --lectures 1-9 --audio-only --to ./out
```

`--audio-only` 模式临时下整个 mp4 (~916 MB/讲) → ffmpeg 抽 m4a (~22 MB) → 删 mp4，需本机预装 ffmpeg。实测：单讲 ≤ 2.5 min / 9 讲 batch ≤ 16 min（V5.F 真机 baseline）。性能优化复盘见 `docs/superpowers/research/2026-05-12-v5-series-retrospective.md`。

教务课表（衍生命令基于 N2154 周次端点 + oldzc bitmask 过滤 + period_clock 时刻 join）：

```bash
sjtu jwc today --grid                       # 今日剩余的课（comfy-table）
sjtu jwc week --zs 14 --grid                # 第 14 周整周课表
sjtu jwc next --within 7 --limit 10 --yaml  # 未来 7 天前 10 节课
sjtu jwc schedule --yaml                    # 整学期课表 (N2151)
```

教务校历导出（`sjtu jwc calendar` 把课表 / 考试 / 学年校历合成 RFC 5545 `.ics`；不带任何输出标志时默认把原始 `.ics` 直接写到 stdout；任一 `--json` / `--yaml` / `--to` 都会触发 envelope 模式 —— 此时 stdout 输出 envelope，`--to` 同时把 raw `.ics` 落盘）：

```bash
sjtu jwc calendar > schedule.ics
sjtu jwc calendar --xnm 2025 --xqm 12 --to ~/Desktop/sjtu.ics
sjtu jwc calendar --no-academic --no-exams > courses.ics
sjtu jwc calendar --to /tmp/cal.ics --json
```

- `--xnm`：学年 4 位；留空则按今天自动推断
- `--xqm`：学期编码；`3`=秋 / `12`=春 / `16`=夏，留空则按今天自动推断
- `--to`：把 `.ics` 写到指定路径；**带 `--to` 会触发 envelope 模式**（stdout 改输出 envelope，raw `.ics` 只去文件）。不传 `--to` 且不带 `--json` / `--yaml` 时原始 `.ics` 才走 stdout
- `--no-academic`：跳过学年校历整天事件
- `--no-exams`：跳过考试事件
- 幂等 UID：每个 VEVENT 的 UID 基于 `<学年>_<学期>_<类型>_<课号>_<...>` 的 FNV-1a 64-bit hash 生成；重复 import 同一份 `.ics` 不应产生重复事件

> **真实约束**（T12 真机暴露）：ZF 9 SP 不再接受空 `xnm`/`xqm` —— CLI 按今天日期推默认（春/秋/夏），调用方可显式 `--xnm 2025 --xqm 12` 覆盖。

教务 GPA + 排名（N309131 两阶段 SP，server 返 `"X/Y"` 字符串 → client 端 `parse_rank` 附加 `gpapmParsed`/`xjfpmParsed` 解析结构）：

```bash
sjtu jwc gpa --scope hxkc --rank njzy --yaml          # 单学期：核心课 + 年级专业排名（推荐）
sjtu jwc gpa --scope qbkc --rank bj                   # 全部课 + 班级排名
sjtu jwc gpa-by-semester --scope hxkc --rank njzy     # 多学期循环（默认当年-3 ~ 当年）
sjtu jwc gpa-by-semester --xnm-from 2022 --xnm-to 2024 --yaml
```

> **注意**：`--rank nj`（纯年级）在部分 SJTU 实例上 server 返 HTML 错误页 → 单学期会 exit 1，多学期版会装进 `failed[]` 不崩。Agent 默认走 `--rank njzy`。N309131 server-side 统计每次 4-5s（不是网络 RTT），12 学期循环真机 ~56s。

一卡通余额 + 消费记录（双轨鉴权，默认 `--via auto` 自动选择路径）：

```bash
# OAuth2 path（需 developer.sjtu.edu.cn 审批的 client_id）
sjtu card auth --client-id <YOUR_CLIENT_ID>   # 弹浏览器同意授权，token 落 sub_sessions/card_oauth.json
sjtu card balance --via oauth2                 # 强制走 OAuth2 path
sjtu card balance --with-identity              # 含学号 / 姓名 / 单位 / 银行卡尾号（仅 oauth2 path）

# weixin path（无需 client_id，用 jaccount cookie 透明跳 OAuth2 拿 weixin.sjtu.edu.cn 数据）
sjtu card balance --via weixin                 # 强制走 weixin path（HTML scrape，无需申请）
sjtu card history --days 7 --via weixin        # 7 天消费记录，weixin path
sjtu card history --days 30 --yaml             # 30 天，YAML 输出（auto 路径）
```

| `--via` | 鉴权 | 数据源 | 适用场景 |
|---|---|---|---|
| `auto`（默认）| 本地有 OAuth2 token → oauth2；否则 weixin | 自动 | 无脑选 |
| `oauth2` | OAuth2 Authorization Code | `api.sjtu.edu.cn` | 已申请 client_id |
| `weixin` | jaccount cookie + HTML scrape | `weixin.sjtu.edu.cn` | 无 client_id 时兜底 |

**Envelope `meta.via`** 字段反映本次实际走的路径，Agent 可据此判断 schema 中可选字段（`--with-identity` 仅 oauth2 path 出 user/bank_no_redacted；weixin path PII 红线永 None）。

- token 自动续期：access_token TTL 30 分钟，refresh_token 透明 refresh，用户无感
- 金额一律 `Decimal` 序列化为字符串（避 JSON f64 精度坑）；total_amount 链式累加精确
- 红线：充值 / 挂失 / 解挂 / 改密码 / 改照片 / 拾卡 等写端点 CLI **永久不实装**

邮箱（mail.sjtu.edu.cn Zimbra，jaccount cookie 透明跳 SSO + ZM_AUTH_TOKEN + `csrf=1:1` flag 强制 `<csrfToken>` envelope）：

```bash
sjtu mail list --limit 50                      # inbox 最近 50 封预览
sjtu mail ls --unread --limit 20               # 仅未读，别名 ls
sjtu mail list --search "通知" --limit 10      # 关键字搜索（自动限定 in:inbox）
sjtu mail read <id> --yaml                     # 读单封正文（text/plain，**不**标已读）
```

- 红线：**永久不实装** 发信 / 存草稿 / 标已读 / 删邮件 / 移动 / 移除标签 / 任何 `*Action` SOAP 类（编译期就找不到入口）
- 单封正文编译期注入 `read="0" html="0" max="50000"`：永不触发已读状态变更、永不取 HTML body、限 50KB 防大附件
- 正文不缓存到磁盘；SOAP 走 1.1 namespace + Content-Type `text/xml; charset=utf-8`

## 技术栈

Rust 2021 / clap 4 / reqwest（H1.1 + rustls）/ tokio / headless_chrome（QR 登录）/ rust_decimal（金额硬约束）/ ffmpeg（可选，Canvas Video `--audio-only` 抽流）。依赖见 `Cargo.toml`。

## 许可

MIT，详见 [LICENSE](LICENSE)。

## 致谢

参考了 `xiaohongshu-cli` 的三级认证与 Envelope 输出契约。
