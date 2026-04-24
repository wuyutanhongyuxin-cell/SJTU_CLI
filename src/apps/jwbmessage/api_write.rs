//! 交我办写端点：目前只有 readall（全部已读）。
//!
//! 端点形状来自 tasks/s3b-jiaowoban-messages.md §2.4（JS bundle 反查）：
//! - POST /api/jwbmessage/message/readall
//! - Content-Type: application/json
//! - X-Requested-With: XMLHttpRequest
//! - Body: `{}`
//! - 2026-04-24 调研时未真发 POST，因此响应形状 `ReadAllResponse` 走宽松。
//!
//! **警告**：这是全局 all-or-nothing，没有按 group / 按单条的 mark-read 端点。
//! CLI 侧必须强制 `--yes` 二次确认。

use anyhow::Result;
use reqwest::header::{ACCEPT, CONTENT_TYPE, REFERER, USER_AGENT};
use reqwest::Client as HttpClient;

use super::http::UA;
use super::models::ReadAllResponse;
use super::throttle::Throttle;
use crate::error::SjtuCliError;

/// POST /api/jwbmessage/message/readall — 把全部未读标记为已读。
pub(super) async fn read_all(
    http: &HttpClient,
    throttle: &Throttle,
    base: &str,
) -> Result<ReadAllResponse> {
    throttle.wait().await;
    let url = format!("{base}/api/jwbmessage/message/readall");
    let resp = http
        .post(&url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, UA)
        .header(REFERER, base)
        .header(CONTENT_TYPE, "application/json")
        .header("X-Requested-With", "XMLHttpRequest")
        .body("{}")
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("POST {url}: {e}")))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("{url}: 读 body: {e}")))?;
    if !status.is_success() {
        return Err(SjtuCliError::UpstreamError(format!(
            "/readall status={status} snippet={}",
            truncate(&body, 200)
        ))
        .into());
    }
    // 后端可能返空体 / `{}` / 结构化 JSON —— 空体退化为默认。
    if body.trim().is_empty() {
        return Ok(ReadAllResponse::default());
    }
    serde_json::from_str(&body).map_err(|e| {
        SjtuCliError::UpstreamError(format!(
            "/readall JSON 解析失败: {e}. snippet={}",
            truncate(&body, 200)
        ))
        .into()
    })
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
