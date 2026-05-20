//! `sjtu mail <sub>` 相关的 clap 枚举 + 派发。
//!
//! MVP 四件套（均**只读**）：
//! - `list` —— inbox 列表（含 --unread / --search / --limit / --offset）
//! - `read <id>` —— 读单封正文（**read=0，不标已读**）
//!
//! 互斥约束：`--unread` 与 `--search` 不能同时用（clap 运行时检查）。

use anyhow::Result;
use clap::Subcommand;

use crate::commands::mail as mail_cmds;
use crate::output::OutputFormat;

/// `sjtu mail <sub>` 的子命令集合。
#[derive(Debug, Subcommand)]
pub enum MailSub {
    /// 列出收件箱邮件（默认 50 条）。支持 --unread 只看未读 / --search 关键字搜索。
    #[command(alias = "ls")]
    List {
        /// 返回条数（默认 50）。
        #[arg(long, default_value_t = mail_cmds::DEFAULT_LIMIT)]
        limit: u32,

        /// 分页偏移（默认 0，第一页）。
        #[arg(long, default_value_t = 0)]
        offset: u32,

        /// 仅显示未读邮件。与 --search 互斥。
        #[arg(long)]
        unread: bool,

        /// 关键字搜索（标题 / 正文预览）。与 --unread 互斥。
        #[arg(long)]
        search: Option<String>,
    },

    /// 读取单封邮件完整正文（**不标已读**）。
    Read {
        /// Zimbra 邮件 ID（从 `sjtu mail list` 的 `id` 字段取）。
        id: String,
    },
}

/// 派发 `sjtu mail <sub>` 到 `commands::mail` handler。
pub async fn dispatch(sub: MailSub, fmt: Option<OutputFormat>) -> Result<()> {
    match sub {
        MailSub::List {
            limit,
            offset,
            unread,
            search,
        } => {
            // 互斥校验：--unread 与 --search 不能同时用。
            if unread && search.is_some() {
                anyhow::bail!("--unread 与 --search 互斥：只能选择其中一个过滤条件");
            }
            if let Some(kw) = search {
                mail_cmds::cmd_mails_search(&kw, limit, offset, fmt).await
            } else if unread {
                mail_cmds::cmd_mails_unread(limit, offset, fmt).await
            } else {
                mail_cmds::cmd_mails(limit, offset, fmt).await
            }
        }
        MailSub::Read { id } => mail_cmds::cmd_mail_read(&id, fmt).await,
    }
}
