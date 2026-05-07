//! `sjtu services <sub>` handler：MVP 仅 `pending`。

use anyhow::Result;

use crate::apps::services::{Client, TodoItem};
use crate::output::{render, Envelope, OutputFormat};

use super::data::PendingData;

/// `sjtu services pending [--with-identity]`：
/// - 拉 `/api/task/me/processes/todo?thing=false`
/// - 按 `code == "ADD"` 分流"我申请的" / "等我处理的"
/// - 默认脱敏：`process.owner.{name,id}` → `***`；`process.name` → `None`
///   （流程标题服务端会塞"工号+真名+业务名"，整字段最干净）
/// - `--with-identity` 对称暴露
pub async fn cmd_pending(with_identity: bool, fmt: Option<OutputFormat>) -> Result<()> {
    let client = Client::connect().await?;
    let (total, mut entities) = client.pending().await?;

    if !with_identity {
        for it in entities.iter_mut() {
            redact_identity(it);
        }
    }

    let returned = entities.len();
    let (my_applications, awaiting_my_action): (Vec<_>, Vec<_>) = entities
        .into_iter()
        .partition(|i| i.code.as_deref() == Some("ADD"));

    let data = PendingData {
        total,
        returned,
        with_identity,
        my_applications,
        awaiting_my_action,
    };
    render(Envelope::ok(data), fmt)
}

/// 默认脱敏：
/// - `process.owner.{name,id}` 替换为 `***`（`account` 保留——与可见 URL 一致，非身份强标识）
/// - `process.name` 整字段移除（服务端塞"工号+真名+业务名"，无法可靠正则剥离；
///   `app.name` + `entry` 已足够定位流程）
fn redact_identity(it: &mut TodoItem) {
    if let Some(p) = it.process.as_mut() {
        if let Some(owner) = p.owner.as_mut() {
            if owner.name.is_some() {
                owner.name = Some("***".to_string());
            }
            if owner.id.is_some() {
                owner.id = Some("***".to_string());
            }
        }
        p.name = None;
    }
}
