//! `sjtu library <sub>` 相关的 clap 枚举 + 派发。
//!
//! MVP 三件套（均**只读**）：
//! - `loans` —— 当前借阅
//! - `history` —— 历史借阅
//! - `fines` —— 罚款（仅显示，不点缴费）
//!
//! 红线契约：docs/superpowers/plans/2026-05-20-t7-library-loans.md。

use anyhow::Result;
use clap::Subcommand;

use crate::commands::library as library_cmds;
use crate::output::OutputFormat;

/// `sjtu library <sub>` 的子命令集合。
#[derive(Debug, Subcommand)]
pub enum LibrarySub {
    /// 当前借阅明细。**只读**。
    Loans,

    /// 历史借阅明细。**只读**。
    History,

    /// 罚款明细（仅显示，不点缴费）。**只读**。
    Fines,
}

/// 派发 `sjtu library <sub>` 到 `commands::library` handler。
pub async fn dispatch(sub: LibrarySub, fmt: Option<OutputFormat>) -> Result<()> {
    match sub {
        LibrarySub::Loans => library_cmds::cmd_loans(fmt).await,
        LibrarySub::History => library_cmds::cmd_history(fmt).await,
        LibrarySub::Fines => library_cmds::cmd_fines(fmt).await,
    }
}
