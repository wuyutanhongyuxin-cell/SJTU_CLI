//! `sjtu shuiyuan <sub>` 的 `#[arg(value_enum)]` 枚举集合。
//!
//! 拆出来以守住 `src/cli/shuiyuan.rs` 的 200 行硬限；本文件只负责
//! 「clap 字符串 → commands 层领域枚举」的映射，没有任何业务逻辑。

use clap::ValueEnum;

use crate::commands::shuiyuan as shuiyuan_cmds;

/// `--render` 参数枚举。与 `commands::shuiyuan::RenderMode` 一一映射。
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum RenderModeArg {
    Markdown,
    Plain,
    Raw,
}

impl From<RenderModeArg> for shuiyuan_cmds::RenderMode {
    fn from(r: RenderModeArg) -> Self {
        match r {
            RenderModeArg::Markdown => Self::Markdown,
            RenderModeArg::Plain => Self::Plain,
            RenderModeArg::Raw => Self::Raw,
        }
    }
}

/// `--in` 参数枚举。与 `commands::shuiyuan::SearchIn` 一一映射。
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SearchInArg {
    All,
    Topic,
    Post,
}

impl From<SearchInArg> for shuiyuan_cmds::SearchIn {
    fn from(s: SearchInArg) -> Self {
        match s {
            SearchInArg::All => Self::All,
            SearchInArg::Topic => Self::Topic,
            SearchInArg::Post => Self::Post,
        }
    }
}

/// `--filter` 参数枚举。与 `commands::shuiyuan::PmFilterCli` 一一映射。
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PmFilterArg {
    Inbox,
    Sent,
    Unread,
    New,
}

impl From<PmFilterArg> for shuiyuan_cmds::PmFilterCli {
    fn from(f: PmFilterArg) -> Self {
        match f {
            PmFilterArg::Inbox => Self::Inbox,
            PmFilterArg::Sent => Self::Sent,
            PmFilterArg::Unread => Self::Unread,
            PmFilterArg::New => Self::New,
        }
    }
}
