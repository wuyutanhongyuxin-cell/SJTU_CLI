//! clap 命令行入口：定义顶层 `Cli` 与 `Commands` 枚举，并派发到各子命令。
//!
//! Shuiyuan 相关的 clap 枚举 + 派发拆在 `shuiyuan.rs` 里，本文件只保留顶层骨架。

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

mod canvas;
mod canvas_video;
mod card;
mod elec;
mod jwbmessage;
pub mod jwc;
mod library;
mod mail;
mod services;
mod shuiyuan;
mod shuiyuan_args;

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

    /// 水源社区（shuiyuan.sjtu.edu.cn）：只读论坛命令。写操作留给后续子阶段、默认 `--confirm`。
    Shuiyuan {
        #[command(subcommand)]
        sub: shuiyuan::ShuiyuanSub,
    },

    /// 交我办消息中心（my.sjtu.edu.cn）：分组列表 / 组内消息 / 全部已读。
    Messages {
        #[command(subcommand)]
        sub: jwbmessage::MessagesSub,
    },

    /// Canvas LMS（oc.sjtu.edu.cn）：PAT 鉴权的只读作业 DDL 查询。
    Canvas {
        #[command(subcommand)]
        sub: canvas::CanvasSub,
    },

    /// 课堂视频（v.sjtu.edu.cn / 交我学）：LTI 1.3 launch 鉴权的视频列表 / 下载。
    ///
    /// 与上面的 `canvas`（PAT 鉴权 oc.sjtu）独立 —— 鉴权链 / 域名 / 端点全不一样。
    /// CP-V1 仅 `list`；下载留给 CP-V3/V4。
    CanvasVideo {
        #[command(subcommand)]
        sub: canvas_video::CanvasVideoSub,
    },

    /// 教务系统（i.sjtu.edu.cn / 正方）：成绩 / 课表 / GPA / 考试安排（MVP 仅成绩）。
    Jwc {
        #[command(subcommand)]
        sub: jwc::JwcSub,
    },

    /// 图书馆借阅（weijieyue.lib.sjtu.edu.cn:8080）：当前 / 历史 / 罚款（**只读**）。
    Library {
        #[command(subcommand)]
        sub: library::LibrarySub,
    },

    /// 办事大厅（my.sjtu.edu.cn）：流程待办列表（MVP 仅 `pending`，只读）。
    Services {
        #[command(subcommand)]
        sub: services::ServicesSub,
    },

    /// 生活服务 — 宿舍电费（elec.sjtu.edu.cn）：余额 / 月度 / 日明细（MVP 三件套，只读）。
    Elec {
        #[command(subcommand)]
        sub: elec::ElecSub,
    },

    /// 一卡通（api.sjtu.edu.cn/v1/me/card*）：余额 + 消费记录只读查询（OAuth2 鉴权）。
    Card {
        #[command(subcommand)]
        sub: card::CardSub,
    },

    /// 邮件（mail.sjtu.edu.cn Zimbra SOAP）：收件箱列表 / 未读过滤 / 搜索 / 读正文（只读）。
    Mail {
        #[command(subcommand)]
        sub: mail::MailSub,
    },
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
///
/// S2 起改 async：`cas` 路径需要 await reqwest；其余子命令仍是同步实现，
/// 在 async 上下文里直接调用即可（不阻塞 reactor，因为是 fs / 子进程类同步 IO）。
pub async fn run() -> Result<()> {
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
        Commands::Shuiyuan { sub } => shuiyuan::dispatch(sub, fmt).await,
        Commands::Messages { sub } => jwbmessage::dispatch(sub, fmt).await,
        Commands::Canvas { sub } => canvas::dispatch(sub, fmt).await,
        Commands::CanvasVideo { sub } => canvas_video::dispatch(sub, fmt).await,
        Commands::Jwc { sub } => jwc::dispatch(sub, fmt).await,
        Commands::Library { sub } => library::dispatch(sub, fmt).await,
        Commands::Services { sub } => services::dispatch(sub, fmt).await,
        Commands::Elec { sub } => elec::dispatch(sub, fmt).await,
        Commands::Card { sub } => card::dispatch(sub, fmt).await,
        Commands::Mail { sub } => mail::dispatch(sub, fmt).await,
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
        stage: "S3a — 水源社区只读命令（代码落盘，真实 checkpoint 待跑）",
        message: "登录已就绪：`sjtu login` 扫码 → `sjtu status` 查 session → `sjtu shuiyuan --help` 看水源只读命令。",
    };
    render(Envelope::ok(data), fmt)
}
