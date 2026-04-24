//! Canvas HTTP Client 构造 + 公共 JSON 请求封装（节流 + 重试 + 401 映射）。
//!
//! 与 shuiyuan / jwbmessage 的 http.rs 同构，差异点：
//! - 鉴权走 Bearer（PAT），不走 cookie jar
//! - 401 专门映射到 `CanvasTokenInvalid`（错误文本精准提示重跑 `sjtu canvas setup`；
//!   Envelope error.code 仍归到 `session_expired`），其他非 2xx 走 `CanvasApi`
//! - `Accept` 头走 Canvas 专属 MIME：`application/json+canvas-string-ids, application/json`
//!   （强制 id 字段返 String，避免 JS number 精度丢失）

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, REFERER, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::Client;

use super::throttle::Throttle;
use crate::error::SjtuCliError;

pub(super) const BASE: &str = "https://oc.sjtu.edu.cn";
pub(super) const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
const ACCEPT_MIME: &str = "application/json+canvas-string-ids, application/json";

/// 注入 Bearer PAT + 固定 header 的 reqwest Client。
pub(super) fn build_http_client(pat: &str) -> Result<Client> {
    let mut default = HeaderMap::new();
    let mut auth = HeaderValue::from_str(&format!("Bearer {pat}")).map_err(|e| {
        SjtuCliError::InvalidInput(format!("PAT 含非法字符，无法拼 Authorization 头: {e}"))
    })?;
    auth.set_sensitive(true); // 日志里不打印
    default.insert(AUTHORIZATION, auth);
    default.insert(ACCEPT, HeaderValue::from_static(ACCEPT_MIME));
    default.insert(USER_AGENT, HeaderValue::from_static(UA));
    default.insert(REFERER, HeaderValue::from_static(BASE));
    default.insert(
        "X-Requested-With",
        HeaderValue::from_static("XMLHttpRequest"),
    );

    Client::builder()
        .default_headers(default)
        .redirect(Policy::limited(5))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .gzip(true)
        .http1_only()
        .pool_idle_timeout(Duration::from_millis(0))
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 HTTP client 失败: {e}")).into())
}

/// 公共 JSON GET：节流 + 固定 header + 连接层错自动重试 1 次 + 错误带 snippet。
///
/// 401 → `SjtuCliError::CanvasTokenInvalid`（触发"重跑 `sjtu canvas setup`"提示）
/// 其他 4xx/5xx → `SjtuCliError::CanvasApi(...)`，snippet 截 200 字
pub(super) async fn fetch_json<T: serde::de::DeserializeOwned>(
    http: &Arc<Client>,
    throttle: &Throttle,
    url: &str,
    label: &str,
) -> Result<T> {
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        throttle.wait().await;
        match fetch_once(http, url, label).await {
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
    Err(last_err.expect("至少一次尝试"))
}

async fn fetch_once<T: serde::de::DeserializeOwned>(
    http: &Arc<Client>,
    url: &str,
    label: &str,
) -> Result<T> {
    let resp = http
        .get(url)
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("GET {url}: {}", chain(&e))))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("{url}: 读 body: {e}")))?;
    if status.as_u16() == 401 {
        return Err(SjtuCliError::CanvasTokenInvalid.into());
    }
    if !status.is_success() {
        return Err(SjtuCliError::CanvasApi(format!(
            "{label} status={status} snippet={}",
            truncate(&body, 200)
        ))
        .into());
    }
    serde_json::from_str::<T>(&body).map_err(|e| {
        SjtuCliError::CanvasApi(format!(
            "{label} JSON 解析失败: {e}. snippet={}",
            truncate(&body, 300)
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

fn chain(e: &(dyn std::error::Error + 'static)) -> String {
    let mut msg = format!("{e}");
    let mut cur = e.source();
    while let Some(src) = cur {
        msg.push_str(&format!(" -> {src}"));
        cur = src.source();
    }
    msg
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
