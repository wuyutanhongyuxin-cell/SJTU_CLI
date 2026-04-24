//! `sjtu shuiyuan <sub>` 相关的 clap 枚举 + 派发。
//!
//! S3a/S3b 的子命令集合。`ValueEnum`（RenderModeArg / SearchInArg / PmFilterArg）
//! 抽到 `shuiyuan_args.rs` 以守住 200 行硬限。

use anyhow::Result;
use clap::Subcommand;

use super::shuiyuan_args::{PmFilterArg, RenderModeArg, SearchInArg};
use crate::commands::shuiyuan as shuiyuan_cmds;
use crate::output::OutputFormat;

/// `sjtu shuiyuan <sub>` 的子命令集合。S3a 只读 + 写 + S3b PM 只读。
#[derive(Debug, Subcommand)]
pub enum ShuiyuanSub {
    /// 列首页 latest 帖子。
    Latest {
        /// 页码，从 0 开始。
        #[arg(long, default_value_t = 0)]
        page: u32,
        /// 返回条数上限（Discourse per_page=30）。
        #[arg(long, default_value_t = 30)]
        limit: u32,
    },

    /// 看单个 topic（含正文 + 前 N 楼）。
    Topic {
        /// topic id（水源 URL `/t/<slug>/<id>` 里的数字）。
        id: u64,
        /// 返回楼层数上限。
        #[arg(long, default_value_t = 20)]
        post_limit: u32,
        /// 正文渲染格式。
        #[arg(long, value_enum, default_value_t = RenderModeArg::Markdown)]
        render: RenderModeArg,
    },

    /// 收件箱（通知）。
    Inbox {
        /// 只显示未读。
        #[arg(long)]
        unread_only: bool,
        /// 条数上限。
        #[arg(long, default_value_t = 30)]
        limit: u32,
    },

    /// 私信列表（Discourse PM）。`--filter` 切换收件/已发/未读/新增。
    Messages {
        /// 过滤器：inbox（默认）/sent/unread/new。
        #[arg(long, value_enum, default_value_t = PmFilterArg::Inbox)]
        filter: PmFilterArg,
        /// 页码，从 0 开始。
        #[arg(long, default_value_t = 0)]
        page: u32,
        /// 返回条数上限（Discourse per_page=30）。
        #[arg(long, default_value_t = 30)]
        limit: u32,
    },

    /// 看单条私信详情。本质上复用 `/t/<id>.json`，所以直接转 `topic` handler。
    Message {
        /// 私信 topic id（PM 也是 topic 的一种，URL 里的数字）。
        id: u64,
        /// 返回楼层数上限。
        #[arg(long, default_value_t = 20)]
        post_limit: u32,
        /// 正文渲染格式。
        #[arg(long, value_enum, default_value_t = RenderModeArg::Markdown)]
        render: RenderModeArg,
    },

    /// 全站搜索。
    Search {
        /// 搜索关键词。
        query: String,
        /// 搜索范围。
        #[arg(long = "in", value_enum, default_value_t = SearchInArg::All)]
        scope: SearchInArg,
    },

    /// 回复已有 topic（写操作，必须 `--yes` 或 TTY 二次确认）。
    Reply {
        /// 目标 topic id。
        topic_id: u64,
        /// 回复正文（Markdown，水源 raw 字段）。
        body: String,
        /// 跳过交互确认。
        #[arg(long)]
        yes: bool,
    },

    /// 点赞指定楼层（写操作）。
    Like {
        /// 被赞的 post id（不是 topic id，是具体楼层的 id）。
        post_id: u64,
        #[arg(long)]
        yes: bool,
    },

    /// 发新帖（写操作）。`--category` 可选，不传走水源默认分类。
    NewTopic {
        /// 帖子标题。
        title: String,
        /// 正文（Markdown）。
        body: String,
        /// 目标分类 id。
        #[arg(long)]
        category: Option<u64>,
        #[arg(long)]
        yes: bool,
    },

    /// 发私信给指定用户（写操作）。新开 PM 会话，不是在已有会话里回复。
    PmSend {
        /// 收件人用户名（水源 username，不带 @）。
        to: String,
        /// 私信标题。
        title: String,
        /// 正文（Markdown）。
        body: String,
        #[arg(long)]
        yes: bool,
    },

    /// 删除整条 topic（写操作，**不可恢复**）。首楼删除等同删除整条帖子。
    DeleteTopic {
        /// 要删除的 topic id。
        topic_id: u64,
        #[arg(long)]
        yes: bool,
    },

    /// 删除指定楼层（写操作）。首楼请用 `delete-topic`。
    DeletePost {
        /// 要删除的 post id（具体楼层的 id，不是 topic id）。
        post_id: u64,
        #[arg(long)]
        yes: bool,
    },

    /// 内部调试：探活当前水源 session 是否有效（`/session/current.json`）。
    #[command(hide = true)]
    LoginProbe,
}

/// 派发 `sjtu shuiyuan <sub>` 到 `commands::shuiyuan` 的具体 handler。
pub async fn dispatch(sub: ShuiyuanSub, fmt: Option<OutputFormat>) -> Result<()> {
    match sub {
        ShuiyuanSub::Latest { page, limit } => shuiyuan_cmds::cmd_latest(page, limit, fmt).await,
        ShuiyuanSub::Topic {
            id,
            post_limit,
            render,
        } => shuiyuan_cmds::cmd_topic(id, post_limit, render.into(), fmt).await,
        ShuiyuanSub::Inbox { unread_only, limit } => {
            shuiyuan_cmds::cmd_inbox(unread_only, limit, fmt).await
        }
        ShuiyuanSub::Messages {
            filter,
            page,
            limit,
        } => shuiyuan_cmds::cmd_messages(filter.into(), page, limit, fmt).await,
        ShuiyuanSub::Message {
            id,
            post_limit,
            render,
        } => shuiyuan_cmds::cmd_topic(id, post_limit, render.into(), fmt).await,
        ShuiyuanSub::Search { query, scope } => {
            shuiyuan_cmds::cmd_search(query, scope.into(), fmt).await
        }
        ShuiyuanSub::Reply {
            topic_id,
            body,
            yes,
        } => shuiyuan_cmds::cmd_reply(topic_id, body, yes, fmt).await,
        ShuiyuanSub::Like { post_id, yes } => shuiyuan_cmds::cmd_like(post_id, yes, fmt).await,
        ShuiyuanSub::NewTopic {
            title,
            body,
            category,
            yes,
        } => shuiyuan_cmds::cmd_new_topic(category, title, body, yes, fmt).await,
        ShuiyuanSub::PmSend {
            to,
            title,
            body,
            yes,
        } => shuiyuan_cmds::cmd_pm_send(to, title, body, yes, fmt).await,
        ShuiyuanSub::DeleteTopic { topic_id, yes } => {
            shuiyuan_cmds::cmd_delete_topic(topic_id, yes, fmt).await
        }
        ShuiyuanSub::DeletePost { post_id, yes } => {
            shuiyuan_cmds::cmd_delete_post(post_id, yes, fmt).await
        }
        ShuiyuanSub::LoginProbe => shuiyuan_cmds::cmd_login_probe(fmt).await,
    }
}
