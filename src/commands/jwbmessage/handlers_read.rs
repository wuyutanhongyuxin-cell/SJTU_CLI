//! `sjtu messages <sub>` 的只读 handler：list / show。
//!
//! 注意 `show` 非严格意义的只读 —— GET /messagelist 会**静默标记该组已读**
//! （调研 §2.5）。CLI 侧通过 `side_effect_marked_read: true` 明示给 Agent/用户。

use anyhow::Result;

use crate::apps::jwbmessage::Client;
use crate::output::{render, Envelope, OutputFormat};

use super::data::{ListData, ShowData};

/// `sjtu messages list [--unread-only] [--page N] [--limit N]`：拉分组列表（不触发已读）。
pub async fn cmd_list(
    unread_only: bool,
    page: u32,
    limit: u32,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let client = Client::connect().await?;
    // `include_read=true` 与前端 `read=false` 的字面语义相反但行为一致（返回含已读 15 条）。
    // 为避免歧义这里总是传 true 拿全量，客户端按 `--unread-only` 过滤。
    let (total, mut groups) = client.groups(page, limit, true).await?;
    if unread_only {
        groups.retain(|g| g.unread_num > 0);
    }
    let data = ListData {
        page,
        unread_only,
        returned: groups.len(),
        total,
        groups,
    };
    render(Envelope::ok(data), fmt)
}

/// `sjtu messages show <group-id> [--is-group] [--all] [--page N] [--limit N]`：
/// 拉某分组的消息。**会触发服务端将该组未读标记为已读**。
pub async fn cmd_show(
    group_id: String,
    is_group: bool,
    include_read: bool,
    page: u32,
    limit: u32,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let client = Client::connect().await?;
    let (total, messages) = client
        .messages(&group_id, is_group, page, limit, include_read)
        .await?;
    let data = ShowData {
        group_id,
        is_group,
        include_read,
        returned: messages.len(),
        total,
        side_effect_marked_read: true,
        messages,
    };
    render(Envelope::ok(data), fmt)
}
