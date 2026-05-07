//! `sjtu services <sub>` 相关的 clap 枚举 + 派发。
//!
//! MVP 命令清单（仅 1 条）：
//! - `pending [--with-identity]` —— 待办列表（按 `code=="ADD"` 拆两组）
//!
//! Phase-2（CP-D4 真机后）：`history`（completed 端点）/ `cc`（抄送端点）/ `search <kw>`。

use anyhow::Result;
use clap::Subcommand;

use crate::commands::services as svc_cmds;
use crate::output::OutputFormat;

/// `sjtu services <sub>` 的子命令集合。
#[derive(Debug, Subcommand)]
pub enum ServicesSub {
    /// 列待办事项。**只读**，不会触发任何流程状态变更。
    ///
    /// 输出按 `code` 分两组：
    /// - `my_applications` —— 用户自己发起、当前在某步骤等待的流程
    /// - `awaiting_my_action` —— 等用户审核 / 签字 / 处理的流程
    Pending {
        /// 暴露 `process.owner.{name,id}` 身份字段（默认脱敏为 `***`）。
        #[arg(long)]
        with_identity: bool,
    },
}

/// 派发 `sjtu services <sub>` 到 `commands::services` 的 handler。
pub async fn dispatch(sub: ServicesSub, fmt: Option<OutputFormat>) -> Result<()> {
    match sub {
        ServicesSub::Pending { with_identity } => svc_cmds::cmd_pending(with_identity, fmt).await,
    }
}
