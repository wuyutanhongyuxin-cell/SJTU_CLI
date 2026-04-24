//! `sjtu messages <sub>` 的 handler：read-all。
//!
//! read-all 是交我办唯一的 mark-read 写端点，**全局 all-or-nothing**（无按组 / 按单条）。
//! 因此即使传 `--yes`，也建议脚本谨慎使用。

use anyhow::Result;

use crate::apps::jwbmessage::Client;
use crate::output::{render, Envelope, OutputFormat};
use crate::util::confirm::confirm;

use super::data::ReadAllData;

/// `sjtu messages read-all [--yes]`：一次性把**所有**未读标记为已读。
pub async fn cmd_read_all(assume_yes: bool, fmt: Option<OutputFormat>) -> Result<()> {
    confirm(
        "把**全部**未读消息一次性标记为已读（全局；无法按分组撤销）",
        assume_yes,
    )?;
    let client = Client::connect().await?;
    let response = client.read_all().await?;
    render(
        Envelope::ok(ReadAllData {
            marked: true,
            response,
        }),
        fmt,
    )
}
