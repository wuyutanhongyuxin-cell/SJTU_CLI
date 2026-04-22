//! clap 命令行入口：定义顶层 `Cli` 与 `Commands` 枚举，并派发到各子命令。

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

use crate::auth;
use crate::commands::auth_cmds;
use crate::output::{render, Envelope, OutputFormat};

/// SJTU-CLI 顶层参数。
#[derive(Debug, Parser)]
#[command(
    name = "sjtu",
    version = crate::VERSION,
    about = "SJTU JAccount 命令行工具：扫码登录后一行命令查课表 / 成绩 / 一卡通 / 通知 / Canvas",
    long_about = None
)]
pub struct Cli {
    /// 以 YAML 输出（非 TTY 时默认也是 YAML）。
    #[arg(long, global = true, conflicts_with = "json")]
    yaml: bool,

    /// 以 JSON 输出。
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

/// 所有顶层子命令。
#[derive(Debug, Subcommand)]
enum Commands {
    /// 健康自检：打印项目信息，验证 Envelope 输出链路。
    Hello,

    /// JAccount 扫码登录，cookie 落盘 `~/.sjtu-cli/session.json`。
    Login {
        /// 登录后端：`chrome` 弹出可见浏览器扫码；`rookie` 从本机已登录的浏览器 cookie 库兜底读。
        #[arg(long, value_enum, default_value_t = BackendArg::Chrome)]
        browser: BackendArg,
    },

    /// 清除本地 session.json。幂等。
    Logout,

    /// 查看本地 session 状态（是否登录 / TTL / 脱敏 cookie 摘要）。
    Status,
}

/// 供 clap 解析的 `--browser` 枚举。只为了 derive `ValueEnum`。
#[derive(Debug, Clone, Copy, ValueEnum)]
enum BackendArg {
    Chrome,
    Rookie,
}

impl From<BackendArg> for auth::Backend {
    fn from(b: BackendArg) -> Self {
        match b {
            BackendArg::Chrome => auth::Backend::Chrome,
            BackendArg::Rookie => auth::Backend::Rookie,
        }
    }
}

/// 程序入口：解析参数 → 派发到具体子命令。
pub fn run() -> Result<()> {
    let cli = Cli::parse();

    let fmt = if cli.yaml {
        Some(OutputFormat::Yaml)
    } else if cli.json {
        Some(OutputFormat::Json)
    } else {
        None
    };

    match cli.command {
        Commands::Hello => cmd_hello(fmt),
        Commands::Login { browser } => auth_cmds::cmd_login(browser.into(), fmt),
        Commands::Logout => auth_cmds::cmd_logout(fmt),
        Commands::Status => auth_cmds::cmd_status(fmt),
    }
}

/// `sjtu hello` 返回的数据结构。
#[derive(Debug, Serialize)]
struct HelloData {
    project: &'static str,
    version: &'static str,
    stage: &'static str,
    message: &'static str,
}

/// `sjtu hello` 实现：返回项目自检信息并渲染 Envelope。
fn cmd_hello(fmt: Option<OutputFormat>) -> Result<()> {
    let data = HelloData {
        project: "sjtu-cli",
        version: crate::VERSION,
        stage: "S1 — QR 扫码登录",
        message: "登录已就绪：`sjtu login` 扫码 → `sjtu status` 查 session。",
    };
    render(Envelope::ok(data), fmt)
}
