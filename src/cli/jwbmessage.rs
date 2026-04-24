//! `sjtu messages <sub>` 相关的 clap 枚举 + 派发。
//!
//! 命令清单（仅 3 条）：
//! - `list`       —— 分组列表（不触发已读）
//! - `show <id>`  —— 组内消息（⚠ 会静默标记已读）
//! - `read-all`   —— 全部已读（强制 `--yes`）
//!
//! 未做（端点不存在）：单条 mark-read / 按组 mark-read。

use anyhow::Result;
use clap::Subcommand;

use crate::commands::jwbmessage as jwb_cmds;
use crate::output::OutputFormat;

/// `sjtu messages <sub>` 的子命令集合。
#[derive(Debug, Subcommand)]
pub enum MessagesSub {
    /// 列所有分组。`--unread-only` 仅显示有未读的；**不会**触发已读副作用。
    List {
        /// 只显示有未读的分组（客户端过滤）。
        #[arg(long)]
        unread_only: bool,
        /// 页码，从 0 开始。
        #[arg(long, default_value_t = 0)]
        page: u32,
        /// 每页条数（默认 30）。
        #[arg(long, default_value_t = 30)]
        limit: u32,
    },

    /// 查看某个分组的消息列表。**⚠ 会把该组所有未读静默标记为已读**。
    Show {
        /// 目标分组 id（`list` 返回的 `group_id`）。
        group_id: String,
        /// 目标是否为合并分组（`list` 的 `is_group` 字段）。大多数 App 为 false。
        #[arg(long)]
        is_group: bool,
        /// 连已读消息一并返回（默认服务端只返未读 + 最近已读）。
        #[arg(long)]
        all: bool,
        /// 页码，从 0 开始。
        #[arg(long, default_value_t = 0)]
        page: u32,
        /// 每页条数。
        #[arg(long, default_value_t = 10)]
        limit: u32,
    },

    /// 把**所有**未读消息一次性标记为已读（全局；无法按组撤销）。
    ReadAll {
        /// 跳过交互确认（非 TTY 环境必须显式传此 flag）。
        #[arg(long)]
        yes: bool,
    },
}

/// 派发 `sjtu messages <sub>` 到 `commands::jwbmessage` 的 handler。
pub async fn dispatch(sub: MessagesSub, fmt: Option<OutputFormat>) -> Result<()> {
    match sub {
        MessagesSub::List {
            unread_only,
            page,
            limit,
        } => jwb_cmds::cmd_list(unread_only, page, limit, fmt).await,
        MessagesSub::Show {
            group_id,
            is_group,
            all,
            page,
            limit,
        } => jwb_cmds::cmd_show(group_id, is_group, all, page, limit, fmt).await,
        MessagesSub::ReadAll { yes } => jwb_cmds::cmd_read_all(yes, fmt).await,
    }
}
