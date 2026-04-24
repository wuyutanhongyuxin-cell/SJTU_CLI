//! `sjtu shuiyuan <write>` 的 handler：reply / like / new-topic。
//!
//! 每个命令在真正发 HTTP 之前强制一次 `util::confirm::confirm`：
//! - `--yes` 绕过 prompt；
//! - 非 TTY + 未传 `--yes` 直接硬失败（见 `util::confirm` 注释）。
//!
//! 错误路径：用户取消 → `anyhow::Error`，由 `main` 退码。

use anyhow::Result;

use crate::apps::shuiyuan::Client;
use crate::output::{render, Envelope, OutputFormat};
use crate::util::confirm::confirm;

use super::data::{DeletePostData, DeleteTopicData, LikeData, NewTopicData, PmSendData, ReplyData};

/// `sjtu shuiyuan reply <topic_id> <body> [--yes]`。
pub async fn cmd_reply(
    topic_id: u64,
    body: String,
    assume_yes: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    confirm(
        &format!(
            "在 topic {topic_id} 下回复一楼（{} 字）",
            body.chars().count()
        ),
        assume_yes,
    )?;
    let client = Client::connect().await?;
    let post = client.reply(topic_id, &body).await?;
    render(Envelope::ok(ReplyData { topic_id, post }), fmt)
}

/// `sjtu shuiyuan like <post_id> [--yes]`。
pub async fn cmd_like(post_id: u64, assume_yes: bool, fmt: Option<OutputFormat>) -> Result<()> {
    confirm(&format!("点赞 post {post_id}"), assume_yes)?;
    let client = Client::connect().await?;
    let result = client.like(post_id).await?;
    render(Envelope::ok(LikeData { post_id, result }), fmt)
}

/// `sjtu shuiyuan new-topic <title> <body> [--category N] [--yes]`。
pub async fn cmd_new_topic(
    category: Option<u64>,
    title: String,
    body: String,
    assume_yes: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    confirm(
        &format!(
            "发新帖《{}》（{} 字）到 category {}",
            title,
            body.chars().count(),
            category
                .map(|c| c.to_string())
                .unwrap_or_else(|| "默认".into())
        ),
        assume_yes,
    )?;
    let client = Client::connect().await?;
    let post = client.new_topic(category, &title, &body).await?;
    render(
        Envelope::ok(NewTopicData {
            title,
            category,
            post,
        }),
        fmt,
    )
}

/// `sjtu shuiyuan pm-send <to> <title> <body> [--yes]`：发私信给指定用户（新开会话）。
pub async fn cmd_pm_send(
    to: String,
    title: String,
    body: String,
    assume_yes: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    confirm(
        &format!("发私信给 @{to}：《{title}》（{} 字）", body.chars().count()),
        assume_yes,
    )?;
    let client = Client::connect().await?;
    let post = client.pm_send(&to, &title, &body).await?;
    render(Envelope::ok(PmSendData { to, title, post }), fmt)
}

/// `sjtu shuiyuan delete-topic <topic_id> [--yes]`：删除整条 topic（**不可恢复**）。
pub async fn cmd_delete_topic(
    topic_id: u64,
    assume_yes: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    confirm(
        &format!("删除 topic {topic_id}（**不可恢复**，整条帖子及所有回复一起删）"),
        assume_yes,
    )?;
    let client = Client::connect().await?;
    client.delete_topic(topic_id).await?;
    render(
        Envelope::ok(DeleteTopicData {
            topic_id,
            deleted: true,
        }),
        fmt,
    )
}

/// `sjtu shuiyuan delete-post <post_id> [--yes]`：删除单楼。首楼请用 `delete-topic`。
pub async fn cmd_delete_post(
    post_id: u64,
    assume_yes: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    confirm(
        &format!("删除 post {post_id}（整楼删除，首楼请改用 delete-topic）"),
        assume_yes,
    )?;
    let client = Client::connect().await?;
    client.delete_post(post_id).await?;
    render(
        Envelope::ok(DeletePostData {
            post_id,
            deleted: true,
        }),
        fmt,
    )
}
