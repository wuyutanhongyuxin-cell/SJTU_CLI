//! `application/x-www-form-urlencoded` 形态的 POST（getVodVideoInfos 用）。
//!
//! 与 `http::post_json` 同构：节流 + 一次重试（仅在网络层临时错误） + 业务码失败带 snippet。
//! 拆出独立文件是因为 `http.rs` 已贴 156 行，再加 form 兄弟函数会爆 200 限。

use std::time::Duration;

use anyhow::Result;
use reqwest::header::{ACCEPT, REFERER, USER_AGENT};
use reqwest::Client as HttpClient;

use super::http::{SPA_REFERER, UA};
use super::throttle::Throttle;
use crate::error::SjtuCliError;

/// urlencoded POST，与 `post_json` 同样的重试 / 节流 / 错误规范。
pub(super) async fn post_form<T: serde::de::DeserializeOwned>(
    http: &HttpClient,
    throttle: &Throttle,
    url: &str,
    token: &str,
    body: &str,
    label: &str,
) -> Result<T> {
    let mut last: Option<anyhow::Error> = None;
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        throttle.wait().await;
        match post_form_once::<T>(http, url, token, body, label).await {
            Ok(v) => return Ok(v),
            Err(e) => {
                let msg = format!("{e:#}");
                if !is_retriable(&msg) {
                    return Err(e);
                }
                last = Some(e);
            }
        }
    }
    Err(last.expect("≥1 次"))
}

async fn post_form_once<T: serde::de::DeserializeOwned>(
    http: &HttpClient,
    url: &str,
    token: &str,
    body: &str,
    label: &str,
) -> Result<T> {
    let resp = http
        .post(url)
        .header(ACCEPT, "application/json, text/plain, */*")
        .header(USER_AGENT, UA)
        .header(REFERER, SPA_REFERER)
        .header("token", token)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("POST {url}: {e}")))?;
    let st = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("{url}: read body: {e}")))?;
    if !st.is_success() {
        return Err(SjtuCliError::UpstreamError(format!(
            "{label} status={st} snippet={}",
            truncate(&text, 200)
        ))
        .into());
    }
    serde_json::from_str::<T>(&text).map_err(|e| {
        SjtuCliError::UpstreamError(format!(
            "{label} JSON 解析失败: {e}. snippet={}",
            truncate(&text, 300)
        ))
        .into()
    })
}

fn is_retriable(msg: &str) -> bool {
    msg.contains("operation timed out")
        || msg.contains("error sending request")
        || msg.contains("connection closed")
        || msg.contains("connection reset")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
