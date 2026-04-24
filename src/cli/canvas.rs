//! `sjtu canvas <sub>` 的 clap 枚举 + 派发。
//!
//! 命令清单（仅 4 条，MVP）：
//! - `setup`         —— 交互式粘贴 PAT 落盘
//! - `whoami`        —— 验证 PAT 有效 + 返回身份信息
//! - `today`         —— 今日作业 DDL
//! - `upcoming`      —— 未来 N 天作业 DDL（默认 14）
//!
//! 未做（Phase 2）：courses / assignments / inbox / announcements / 写操作。

use anyhow::Result;
use clap::Subcommand;

use crate::commands::canvas as cv_cmds;
use crate::output::OutputFormat;

/// `sjtu canvas <sub>` 的子命令集合。
#[derive(Debug, Subcommand)]
pub enum CanvasSub {
    /// 交互式粘贴 PAT，落盘到 `<config_dir>/sub_sessions/canvas_token.txt`（600）。
    Setup,

    /// 验证 PAT 有效 + 打印 login_id / time_zone / effective_locale。
    Whoami,

    /// 今日作业（本地 00:00 → 次日 00:00 窗口）。
    Today {
        /// 连已完成的也显示（默认只显未交未评）。
        #[arg(long)]
        include_done: bool,
    },

    /// 未来 N 天作业（本地 00:00 → +N 天 窗口）。
    Upcoming {
        /// 窗口天数（默认 14）。
        #[arg(long, default_value_t = 14)]
        days: u32,
        /// 连已完成的也显示（默认只显未交未评）。
        #[arg(long)]
        include_done: bool,
    },
}

/// 派发 `sjtu canvas <sub>` 到 `commands::canvas` 的 handler。
pub async fn dispatch(sub: CanvasSub, fmt: Option<OutputFormat>) -> Result<()> {
    match sub {
        CanvasSub::Setup => cv_cmds::cmd_setup(fmt).await,
        CanvasSub::Whoami => cv_cmds::cmd_whoami(fmt).await,
        CanvasSub::Today { include_done } => cv_cmds::cmd_today(include_done, fmt).await,
        CanvasSub::Upcoming { days, include_done } => {
            cv_cmds::cmd_upcoming(days, include_done, fmt).await
        }
    }
}
