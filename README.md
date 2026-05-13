# SJTU-CLI

上海交通大学 JAccount 命令行工具。扫码登录一次，终端里直接查水源 / 交我办消息 / Canvas 作业 DDL，输出支持 YAML / JSON，方便 AI Agent 调用。

> **状态**：早期开发中，API 与命令可能变动。仅供个人合规使用。

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
| `sjtu jwc grades\|schedule\|gpa\|exams\|today\|week\|next` | 教务（i.sjtu.edu.cn）—— N305005 成绩 / N2151 学年学期课表 / N309131 GPA / N358105 考试 / N2154 衍生（今日 / 整周 / 接下来 N 天）；`--grid` comfy-table 表格输出 |
| `sjtu jwc gpa-by-semester` | 多学期 GPA 对比（自动循环 4 年 × 3 学期 N309131；真机 ~56s） |

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

> **真实约束**（T12 真机暴露）：ZF 9 SP 不再接受空 `xnm`/`xqm` —— CLI 按今天日期推默认（春/秋/夏），调用方可显式 `--xnm 2025 --xqm 12` 覆盖。

## 技术栈

Rust 2021 / clap 4 / reqwest（H1.1 + rustls）/ tokio / headless_chrome（QR 登录）/ rust_decimal（金额硬约束）/ ffmpeg（可选，Canvas Video `--audio-only` 抽流）。依赖见 `Cargo.toml`。

## 许可

MIT，详见 [LICENSE](LICENSE)。

## 致谢

参考了 `xiaohongshu-cli` 的三级认证与 Envelope 输出契约。
