//! `sjtu jwc <sub>` 的只读 handler。MVP 仅 grades。

use anyhow::Result;

use crate::apps::jwc::Client;
use crate::output::{render, Envelope, OutputFormat};

use super::data::GradesData;

/// `sjtu jwc grades [--xnm 2025] [--xqm 3] [--page 1] [--limit 50]`：N305005 成绩查询。
pub async fn cmd_grades(
    xnm: Option<String>,
    xqm: Option<String>,
    page: u32,
    limit: u32,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let client = Client::connect().await?;
    let env_resp = client
        .grades(xnm.as_deref(), xqm.as_deref(), page, limit)
        .await?;

    let returned = env_resp.items.len();
    let data = GradesData {
        xnm,
        xqm,
        page,
        limit,
        total_result: env_resp.total_result,
        total_page: env_resp.total_page,
        returned,
        items: env_resp.items,
    };
    render(Envelope::ok(data), fmt)
}
