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
| `sjtu shuiyuan reply\|like\|new-topic\|delete-*\|pm-send` | 水源写操作（默认 `--confirm`） |
| `sjtu messages list\|show\|read-all` | 交我办消息中心（my.sjtu.edu.cn） |
| `sjtu canvas setup\|whoami\|today\|upcoming` | Canvas LMS（oc.sjtu.edu.cn）作业 DDL |

路线图 / 未完工事项见 `tasks/todo.md`。

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

## 技术栈

Rust 2021 / clap 4 / reqwest / tokio / headless_chrome。依赖见 `Cargo.toml`。

## 许可

MIT（待补 LICENSE 文件）。

## 致谢

参考了 `xiaohongshu-cli` 的三级认证与 Envelope 输出契约。
