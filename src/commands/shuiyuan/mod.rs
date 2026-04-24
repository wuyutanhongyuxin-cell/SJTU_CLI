//! `sjtu shuiyuan <sub>` 子命令实现。
//! - 读：latest / topic / inbox / search + 隐藏 login-probe（`handlers_read`）
//! - 写：reply / like / new-topic（`handlers_write`，都默认 `--yes` 二次确认）
//!
//! 模块组织：
//! - `handlers_read.rs`：5 个读 cmd + `render_post`
//! - `handlers_write.rs`：3 个写 cmd（依赖 `util::confirm`）
//! - `data.rs`：Envelope 里承载的 Data struct（读写共用一个文件）
//! - 本文件：对外导出 + CLI 暴露的 `RenderMode` / `SearchIn` 枚举

mod data;
mod handlers_read;
mod handlers_write;

pub use handlers_read::{
    cmd_inbox, cmd_latest, cmd_login_probe, cmd_messages, cmd_search, cmd_topic,
};
pub use handlers_write::{
    cmd_delete_post, cmd_delete_topic, cmd_like, cmd_new_topic, cmd_pm_send, cmd_reply,
};

use crate::apps::shuiyuan::{PmFilter, SearchScope};

/// CLI 暴露的渲染模式。
#[derive(Debug, Clone, Copy)]
pub enum RenderMode {
    Markdown,
    Plain,
    Raw,
}

impl RenderMode {
    fn as_str(self) -> &'static str {
        match self {
            RenderMode::Markdown => "markdown",
            RenderMode::Plain => "plain",
            RenderMode::Raw => "raw",
        }
    }
}

/// CLI 暴露的搜索范围。
#[derive(Debug, Clone, Copy)]
pub enum SearchIn {
    All,
    Topic,
    Post,
}

impl SearchIn {
    fn as_str(self) -> &'static str {
        match self {
            SearchIn::All => "all",
            SearchIn::Topic => "topic",
            SearchIn::Post => "post",
        }
    }
}

impl From<SearchIn> for SearchScope {
    fn from(s: SearchIn) -> Self {
        match s {
            SearchIn::All => SearchScope::All,
            SearchIn::Topic => SearchScope::Topic,
            SearchIn::Post => SearchScope::Post,
        }
    }
}

/// CLI 暴露的私信过滤器。与 `apps::shuiyuan::PmFilter` 一一映射。
#[derive(Debug, Clone, Copy)]
pub enum PmFilterCli {
    Inbox,
    Sent,
    Unread,
    New,
}

impl PmFilterCli {
    fn as_str(self) -> &'static str {
        match self {
            PmFilterCli::Inbox => "inbox",
            PmFilterCli::Sent => "sent",
            PmFilterCli::Unread => "unread",
            PmFilterCli::New => "new",
        }
    }
}

impl From<PmFilterCli> for PmFilter {
    fn from(f: PmFilterCli) -> Self {
        match f {
            PmFilterCli::Inbox => PmFilter::Inbox,
            PmFilterCli::Sent => PmFilter::Sent,
            PmFilterCli::Unread => PmFilter::Unread,
            PmFilterCli::New => PmFilter::New,
        }
    }
}
