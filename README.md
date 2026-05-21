<div align="center">

# 🐾 SJTU-CLI

**上海交通大学 JAccount 命令行工具 —— 终端里查课表、看作业、追水源**

[![Status](https://img.shields.io/badge/status-alpha-orange)](#)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange?logo=rust)](https://www.rust-lang.org/)
[![Subsystems](https://img.shields.io/badge/subsystems-9-brightgreen)](#-命令速查)
[![Mode](https://img.shields.io/badge/mode-read--only-success)](#%EF%B8%8F-合规与隐私)
[![Platform](https://img.shields.io/badge/platform-Win%20%7C%20macOS%20%7C%20Linux-lightgrey)](#-30-秒上手)

🦊 扫码登录一次 · 🐱 一行命令搞定 · 🐰 输出可喂给 AI Agent

</div>

---

## 🎯 我能用它干什么

不用打开浏览器、不用反复登 9 个子系统，终端里一行命令搞定：

| 想做的事 | 一行命令 |
|---|---|
| 🌅 早八前 30 秒看今天有几节课 | `sjtu jwc today --grid` |
| 📅 周末规划下周课表 | `sjtu jwc week --grid` |
| 📊 看这学期成绩 / GPA | `sjtu jwc grades` ／ `sjtu jwc gpa` |
| 💰 查一卡通余额 + 最近消费 | `sjtu card balance` |
| ⚡ 查宿舍电费余额 | `sjtu elec balance` |
| 📨 看交我办未读消息 | `sjtu messages list --unread-only` |
| 📚 看图书馆借了什么书 | `sjtu library loans` |
| ✉️ 看邮箱未读邮件 | `sjtu mail ls --unread` |
| 🎥 批量下 Canvas 课堂视频 | `sjtu canvas-video download <课程ID>` |
| ⏰ Canvas 作业 DDL 提醒 | `sjtu canvas upcoming --days 14` |
| 🌊 刷水源最新 | `sjtu shuiyuan latest --limit 10` |
| 📆 课表导成 `.ics` 喂手机日历 | `sjtu jwc calendar > schedule.ics` |

> 任何只读命令加 `--yaml` 或 `--json` 都返回结构化输出，方便接 AI Agent。

---

## ⚡ 30 秒上手

```bash
# 1. 编译（首次约 3 分钟）
git clone https://github.com/wuyutanhongyuxin-cell/SJTU_CLI.git
cd SJTU_CLI && cargo build --release

# 2. 扫码登录（弹浏览器，手机 JAccount 扫一下）
./target/release/sjtu login

# 3. 试一下
./target/release/sjtu jwc today --grid
```

> 💡 想全局调用：把 `target/release/sjtu` 加进 PATH，之后直接 `sjtu xxx` 即可。

---

## 🐾 命令速查

### 🌱 日常只读

| 命令 | 做什么 |
|---|---|
| `sjtu login` ／ `status` ／ `logout` | 扫码登录 / 查登录状态 / 注销 |
| `sjtu jwc today` ／ `week` ／ `next` | 今日剩余课 / 整周课表 / 未来 N 天 |
| `sjtu jwc grades` ／ `gpa` ／ `exams` | 成绩单 / GPA + 排名 / 考试安排 |
| `sjtu jwc calendar` | 课表 + 考试 + 校历 → `.ics` 导手机日历 |
| `sjtu card balance` ／ `history` | 一卡通余额 / 消费记录 |
| `sjtu elec balance` ／ `usage` ／ `history` | 宿舍电费余额 / 用量 / 历史 |
| `sjtu mail list` ／ `read` | 邮箱预览 / 读单封（绝不标已读） |
| `sjtu messages list` | 交我办消息中心 |
| `sjtu services pending` | 办事大厅待办 / 已办 / 抄送 |
| `sjtu canvas upcoming` ／ `today` | Canvas 作业 DDL |
| `sjtu canvas-video list` ／ `download` | Canvas 课堂视频清单 / 批量下载 |
| `sjtu library loans` ／ `history` ／ `fines` | 在借书 / 借阅历史 / 罚款 |
| `sjtu shuiyuan latest` ／ `inbox` ／ `search` | 水源最新帖 / 私信 / 搜索 |

### 🐉 写操作（默认 `--confirm` 二次确认）

| 命令 | 做什么 |
|---|---|
| `sjtu shuiyuan reply` ／ `like` ／ `new-topic` | 水源回帖 / 点赞 / 发新帖 |
| `sjtu shuiyuan pm-send` ／ `archive-pm` | 私信发送 / 归档 |
| `sjtu canvas setup` | 配置 Canvas Personal Access Token |
| `sjtu card auth` | 一卡通 OAuth2 授权（高级路径） |

### 🦊 给 AI Agent 用

```bash
sjtu jwc next --within 7 --limit 5 --yaml
sjtu card balance --json | jq '.data.balance'
```

输出 envelope 契约见 [SCHEMA.md](SCHEMA.md)，AI Agent 使用指南见 [SKILL.md](SKILL.md)。

---

## 🛡️ 合规与隐私

🦔 **只读优先**
- 默认全部命令只读；写操作必须显式 `--confirm`
- **永远不实装**：抢课 / 代登录 / 批量爬他人 / 一卡通充值挂失 / 邮件发送删除 / 课程加退选 / 个人信息修改
- 邮箱 / 一卡通 / 图书馆等子系统的"会改状态"端点在源码层就找不到入口

🐢 **本机离线**
- session / token 只落本机 `~/.sjtu-cli/`（Unix 权限 600）
- cookie / 学号 / 姓名 永不入 git、永不上报
- 日志脱敏：cookie 只打前 8 位 + `***`

🐼 **数据精度**
- 一卡通 / 电费金额一律字符串 Decimal，绝不走 float（防 JSON 精度坑）

🦋 **数据不外流 · 严格遵规**
- 所有 SJTU 数据流**本机直连**官方域名（`*.sjtu.edu.cn`），**绝不经第三方代理 / 中转站 / 加速器 / 镜像站 / 云函数**转发
- 不上传 cookie / token / 学号 / 姓名 / 成绩 / 一卡通余额 / 邮件正文 到任何外部服务（含云端 LLM、AI 推理网关、公开 pastebin、Discord / Telegram bot 等）
- 本工具默认**不集成任何 LLM 调用**；若你自行把 `--yaml` 输出喂给 AI 解读，喂什么由你自己决定，请先脱敏（去掉学号 / 姓名 / 完整账单等 PII）
- 严格遵守《上海交通大学计算机网络使用管理办法》及交我办 / 教务 / Canvas / 图书馆 / 邮箱 等各子系统服务条款

请遵守上海交通大学相关服务条款，本工具仅供个人合规使用。

---

## 🔧 开发者细节

<details>
<summary><b>📐 子系统接入方式</b></summary>

| 子系统 | 鉴权 | 数据源 | 备注 |
|---|---|---|---|
| 水源 shuiyuan | jaccount cookie | Discourse REST | 写操作必 `--confirm` |
| 交我办消息 | jaccount cookie | my.sjtu.edu.cn | 只读 |
| Canvas 作业 | Personal Access Token | oc.sjtu.edu.cn | 避 SSO 折腾 |
| Canvas 课堂视频 | LTI 1.3 + jaccount | v.sjtu.edu.cn | `--audio-only` 借 ffmpeg 抽 m4a |
| 办事大厅 | jaccount cookie | my.sjtu.edu.cn | 只读 |
| 电费 | jaccount cookie | elec.sjtu.edu.cn | Decimal 硬约束 |
| 教务 jwc | CAS retry + jaccount | i.sjtu.edu.cn | SP 编号见下 |
| 一卡通 | 双轨 OAuth2 / weixin path | api.sjtu.edu.cn ／ weixin.sjtu.edu.cn | `--via auto` 默认 |
| 图书馆 | jaccount + OAuth dance | weijieyue.lib.sjtu.edu.cn:8080 | 只读，永不实装续借 / 缴费 |
| 邮箱 | jaccount + ZM_AUTH_TOKEN + `csrf=1:1` | mail.sjtu.edu.cn Zimbra | SOAP 1.1，编译期红线注入 |

</details>

<details>
<summary><b>🎓 教务 jwc SP 编号映射</b></summary>

- `N305005` — 成绩单
- `N2151` — 学年学期课表
- `N309131` — GPA + 排名双轨（`gpapmParsed` / `xjfpmParsed`）
- `N358105` — 考试安排
- `N2154` — 周次端点（衍生 today / week / next）

**校历 `.ics` 幂等**：每个 VEVENT 的 UID 基于 `<学年>_<学期>_<类型>_<课号>_<…>` 的 FNV-1a 64-bit hash 生成，重复 import 同一份 `.ics` 不应产生重复事件。

**N309131 排名陷阱**：`--rank nj`（纯年级）在某些实例服务端返 HTML 错误页 → 单学期 exit 1；`gpa-by-semester` 装进 `failed[]` 不崩。Agent 默认走 `--rank njzy`。N309131 服务端统计每次 4–5s，12 学期循环真机 ~56s。

**T12 真机暴露**：ZF 9 SP 不再接受空 `xnm` / `xqm` —— CLI 按今天日期推默认（春 / 秋 / 夏），调用方可显式 `--xnm 2025 --xqm 12` 覆盖。

</details>

<details>
<summary><b>🔐 一卡通双轨鉴权</b></summary>

| `--via` | 鉴权 | 数据源 | 适用 |
|---|---|---|---|
| `auto`（默认）| 本地有 OAuth2 token → oauth2；否则 weixin | 自动 | 无脑选 |
| `oauth2` | OAuth2 Authorization Code | api.sjtu.edu.cn | 已申请 client_id |
| `weixin` | jaccount cookie + HTML scrape | weixin.sjtu.edu.cn | 无 client_id 时兜底 |

- access_token TTL 30 分钟，refresh_token 透明续期，用户无感
- Envelope `meta.via` 反映本次实际路径
- `--with-identity` 仅 oauth2 path 出 user / bank_no_redacted；weixin path PII 红线永 None
- 金额一律 `Decimal` 序列化为字符串，total_amount 链式累加精确
- **编译期红线**：充值 / 挂失 / 解挂 / 改密码 / 改照片 / 拾卡 写端点永久不实装

</details>

<details>
<summary><b>📧 邮箱实现（Zimbra SOAP）</b></summary>

- jaccount cookie 透明跳 SSO → 拿 `ZM_AUTH_TOKEN` → envelope 显式注入 `<authToken>`（关键 trap：cookie 单独不够 → 500 service.AUTH_REQUIRED）
- SOAP envelope 走 1.1 namespace + Content-Type `text/xml; charset=utf-8`
- `csrf=1:1` flag 强制注入 `<csrfToken>`（R7 CP 时发现的 plan-level fix）
- `mail read` 编译期注入 `read="0" html="0" max="50000"`：永不标已读、永不取 HTML body、限 50KB 防大附件
- 正文不缓存到磁盘；ZM_AUTH_TOKEN 日志脱敏
- **编译期硬禁**：SendMsg / SaveDraft / 所有 `*Action` SOAP 类
- quick-xml 0.34 流式解析 SearchResponse / GetMsgResponse / Fault

</details>

<details>
<summary><b>📅 校历 / 课表 .ics 导出参数</b></summary>

```bash
sjtu jwc calendar > schedule.ics                                      # 原始 .ics → stdout
sjtu jwc calendar --xnm 2025 --xqm 12 --to ~/Desktop/sjtu.ics         # 春学期到桌面
sjtu jwc calendar --no-academic --no-exams > courses.ics              # 只要课表
sjtu jwc calendar --to /tmp/cal.ics --json                            # envelope 模式
```

- `--xnm` 学年 4 位 / `--xqm` `3`=秋 / `12`=春 / `16`=夏；不传按今天推断
- 任一 `--json` / `--yaml` / `--to` 触发 envelope 模式 —— stdout 走 envelope，`.ics` 落 `--to`
- `--no-academic` 跳过整天校历事件 / `--no-exams` 跳过考试

</details>

<details>
<summary><b>🎥 Canvas 课堂视频 `--audio-only`</b></summary>

LTI 1.3 鉴权 → list → 单讲 / 批量 mp4 → ffmpeg 抽 m4a 流。

```bash
sjtu canvas-video list <COURSE_ID> --tool-id <TOOL_ID> --yaml
sjtu canvas-video download <COURSE_ID> --tool-id <TOOL_ID> --lectures 1-9 --audio-only --to ./out
```

- `--audio-only` 临时下整 mp4（~916 MB/讲）→ ffmpeg 抽 m4a（~22 MB）→ 删 mp4，需预装 ffmpeg
- 实测：单讲 ≤ 2.5 min / 9 讲 batch ≤ 16 min（V5.F 真机 baseline）
- 性能复盘见 [`docs/superpowers/research/2026-05-12-v5-series-retrospective.md`](docs/superpowers/research/)

</details>

<details>
<summary><b>🔬 技术栈 & 关键依赖</b></summary>

Rust 2021 / clap 4 / reqwest（HTTP/1.1 + rustls）/ tokio / headless_chrome（QR 登录）/ rust_decimal（金额硬约束）/ scraper 0.21（HTML 解析）/ quick-xml 0.34（SOAP 流式）/ ffmpeg（可选，Canvas Video 抽流）。

完整依赖见 [Cargo.toml](Cargo.toml)。

</details>

<details>
<summary><b>🗺️ 路线图 / 进度 / 教训</b></summary>

- 命令面稳定，仍在持续接入新子系统
- 未完工事项见 [`tasks/todo.md`](tasks/todo.md)
- 性能复盘 / 知识沉淀见 [`docs/superpowers/research/`](docs/superpowers/research/)
- 真机踩坑教训见 [`tasks/lessons.md`](tasks/lessons.md)

</details>

---

## 📜 许可

[MIT](LICENSE)。参考了 `xiaohongshu-cli` 的三级认证与 Envelope 输出契约。

<div align="center">

🦊 Made with Rust · For SJTUers · By SJTUer 🐾

</div>
