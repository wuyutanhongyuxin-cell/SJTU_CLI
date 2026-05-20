//! `sjtu library <sub>` handler：MVP 三件套 loans / history / fines。
//!
//! 都**只读**：load session → Client::connect → 调端点 → 渲染 Envelope。
//!
//! 注：`cookies::load_session()` 在文件不存在时直接返回 `SjtuCliError::NotAuthenticated`，
//! 命令层无需再手动 `.ok_or(...)?`。

use anyhow::Result;

use super::data::{FinesData, HistoryData, LoansData};
use crate::apps::library::Client;
use crate::cookies::load_session;
use crate::output::{render, Envelope, EnvelopeMeta, OutputFormat};

/// `sjtu library loans`：当前借阅。
pub async fn cmd_loans(fmt: Option<OutputFormat>) -> Result<()> {
    let session = load_session()?;
    let client = Client::connect(&session).await?;
    let items = client.loans().await?;
    let data = LoansData {
        count: items.len(),
        items,
    };
    let meta = EnvelopeMeta {
        via: Some("weijieyue".into()),
        source_hint: Some("weijieyue.lib.sjtu.edu.cn:8080".into()),
    };
    render(Envelope::ok_with_meta(data, meta), fmt)
}

/// `sjtu library history`：历史借阅。
pub async fn cmd_history(fmt: Option<OutputFormat>) -> Result<()> {
    let session = load_session()?;
    let client = Client::connect(&session).await?;
    let items = client.history().await?;
    let data = HistoryData {
        count: items.len(),
        items,
    };
    let meta = EnvelopeMeta {
        via: Some("weijieyue".into()),
        source_hint: Some("weijieyue.lib.sjtu.edu.cn:8080".into()),
    };
    render(Envelope::ok_with_meta(data, meta), fmt)
}

/// `sjtu library fines`：罚款（**只显示，不点缴费**）。
pub async fn cmd_fines(fmt: Option<OutputFormat>) -> Result<()> {
    let session = load_session()?;
    let client = Client::connect(&session).await?;
    let items = client.fines().await?;
    let pending_count = items
        .iter()
        .filter(|f| f.status.as_deref() == Some("待缴纳"))
        .count();
    let data = FinesData {
        count: items.len(),
        pending_count,
        items,
    };
    let meta = EnvelopeMeta {
        via: Some("weijieyue".into()),
        source_hint: Some("weijieyue.lib.sjtu.edu.cn:8080".into()),
    };
    render(Envelope::ok_with_meta(data, meta), fmt)
}
