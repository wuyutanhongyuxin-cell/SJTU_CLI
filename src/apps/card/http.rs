//! 一卡通 HTTP client：Bearer 鉴权 + JSON GET + 401/errno=10002 token-expired 识别。
//!
//! 与 `apps/elec/http.rs` 同构骨架，差异：
//! - base URL `https://api.sjtu.edu.cn`
//! - 不带 cookie jar；改 `Authorization: Bearer <token>` 头
//! - errno=10002 / "Authentication Failed" → 上抛 `CardOAuth("token_expired")`，
//!   命令层 with_token_refresh 接住自动 refresh + 重试

use std::time::Duration;

use anyhow::Result;
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::Client;

use super::throttle::Throttle;
use crate::error::SjtuCliError;

pub(super) const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
pub(super) const BASE: &str = "https://api.sjtu.edu.cn";

/// 构造 reqwest Client（无 cookie jar，鉴权走 header）。
pub(super) fn build_http_client() -> Result<Client> {
    Client::builder()
        .redirect(Policy::limited(5))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .gzip(true)
        .http1_only()
        .pool_idle_timeout(Duration::from_millis(0))
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 HTTP client 失败: {e}")).into())
}

/// JSON GET：节流 + Bearer 头 + 重试 1 次（仅连接层错）+ 错误带 snippet。
/// 返回原始 body String —— api.sjtu 的 envelope 解析在 api.rs 处理。
pub(super) async fn fetch_json_raw(
    http: &Client,
    throttle: &Throttle,
    url: &str,
    access_token: &str,
    label: &str,
) -> Result<String> {
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        throttle.wait().await;
        match fetch_once(http, url, access_token, label).await {
            Ok(v) => return Ok(v),
            Err(e) => {
                let msg = format!("{e:#}");
                if !is_retriable(&msg) {
                    return Err(e);
                }
                last_err = Some(e);
            }
        }
    }
    Err(last_err.expect("至少一次尝试的错误"))
}

async fn fetch_once(http: &Client, url: &str, access_token: &str, label: &str) -> Result<String> {
    let resp = http
        .get(url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, UA)
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("GET {url}: {e}")))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("{url}: 读 body: {e}")))?;
    if status.as_u16() == 401 {
        return Err(SjtuCliError::CardOAuth("token_expired".into()).into());
    }
    if !status.is_success() {
        return Err(SjtuCliError::UpstreamError(format!(
            "{label} status={status} snippet={}",
            truncate(&body, 200)
        ))
        .into());
    }
    Ok(body)
}

/// 检查 200 body 是否带 errno=10002 / "Authentication Failed"（spec §5.1 错误形态）。
/// 命中 → 返 `SjtuCliError::CardOAuth("token_expired")`。
pub(super) fn detect_token_expired_in_body(body: &str) -> Option<anyhow::Error> {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(errno) = val.get("errno").and_then(|v| v.as_i64()) {
            if errno == 10002 {
                return Some(SjtuCliError::CardOAuth("token_expired".into()).into());
            }
        }
    }
    if body.contains("\"errno\":10002") || body.contains("Authentication Failed") {
        return Some(SjtuCliError::CardOAuth("token_expired".into()).into());
    }
    None
}

fn is_retriable(msg: &str) -> bool {
    msg.contains("operation timed out")
        || msg.contains("error sending request")
        || msg.contains("connection closed")
        || msg.contains("connection reset")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push_str("...");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_expired_errno_10002_via_json() {
        let body = r#"{"errno":10002,"error":"Authentication Failed","total":0}"#;
        let e = detect_token_expired_in_body(body).expect("应识别 errno=10002");
        let downcasted = e.downcast_ref::<SjtuCliError>();
        assert!(matches!(downcasted, Some(SjtuCliError::CardOAuth(s)) if s == "token_expired"));
    }

    #[test]
    fn detect_expired_substring_fallback() {
        let body = r#"{"errno":10002 garbled... Authentication Failed"#;
        assert!(detect_token_expired_in_body(body).is_some());
    }

    #[test]
    fn detect_no_match_on_normal_body() {
        let body = r#"{"errno":0,"total":1,"entities":[{"cardNo":"X"}]}"#;
        assert!(detect_token_expired_in_body(body).is_none());
    }

    #[test]
    fn detect_no_match_on_4012() {
        let body = r#"{"errno":4012,"error":"other"}"#;
        assert!(detect_token_expired_in_body(body).is_none());
    }

    #[test]
    fn truncate_handles_utf8_multibyte_boundary() {
        // "你好" 是 6 字节（每字 3 byte），按字节切到 max=4 会落在第二字中间
        let s = "你好世界abc";
        let truncated = truncate(s, 3);
        assert_eq!(truncated, "你好世...");
        assert!(!truncate(s, 100).contains("..."));
    }
}
