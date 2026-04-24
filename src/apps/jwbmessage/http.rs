//! 交我办 HTTP Client 构造 + 公共 JSON 请求封装（节流 + 重试 + 错误诊断）。
//!
//! 与 shuiyuan::http 同构，但只注入 `*.sjtu.edu.cn` 域 cookie；域判定走后缀匹配，
//! 因为 CAS 302 链路可能在 `jaccount.sjtu.edu.cn` / `my.sjtu.edu.cn` / `sjtu.edu.cn`
//! 多个子域上种 cookie，全收了 reqwest 按 host 派发就行。
//!
//! 请求头：`X-Requested-With: XMLHttpRequest` + `Accept: application/json` —— 后端
//! 要求两者皆有（无 CSRF，无 Bearer，详见 tasks/s3b-jiaowoban-messages.md §1）。

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::cookie::Jar;
use reqwest::header::{ACCEPT, REFERER, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::Client;
use url::Url;

use super::throttle::Throttle;
use crate::cookies::Session;
use crate::error::SjtuCliError;

pub(super) const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
pub(super) const BASE: &str = "https://my.sjtu.edu.cn";

/// 把 `*.sjtu.edu.cn` 域 cookie 注入 reqwest jar。
pub(super) fn build_http_client(session: &Session) -> Result<Client> {
    let jar = Arc::new(Jar::default());
    let my_url: Url = "https://my.sjtu.edu.cn/".parse().expect("const URL");

    for c in &session.cookies {
        let domain = match c.domain.as_deref() {
            Some(d) if !d.is_empty() => d,
            _ => "my.sjtu.edu.cn",
        };
        if !domain.trim_start_matches('.').ends_with("sjtu.edu.cn") {
            continue;
        }
        let path = c.path.as_deref().unwrap_or("/");
        let s = format!("{}={}; Path={}", c.name, c.value, path);
        jar.add_cookie_str(&s, &my_url);
    }

    Client::builder()
        .cookie_provider(jar)
        .redirect(Policy::limited(5))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .gzip(true)
        .http1_only()
        .pool_idle_timeout(Duration::from_millis(0))
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 HTTP client 失败: {e}")).into())
}

/// 公共 JSON GET：节流 + 标准 header + 连接层错自动重试 1 次 + 错误带 snippet。
pub(super) async fn fetch_json<T: serde::de::DeserializeOwned>(
    http: &Client,
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
    Err(last_err.expect("至少一次尝试的错误"))
}

async fn fetch_once<T: serde::de::DeserializeOwned>(
    http: &Client,
    url: &str,
    label: &str,
) -> Result<T> {
    let resp = http
        .get(url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, UA)
        .header(REFERER, BASE)
        .header("X-Requested-With", "XMLHttpRequest")
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("GET {url}: {}", chain(&e))))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("{url}: 读 body: {e}")))?;
    if !status.is_success() {
        return Err(SjtuCliError::UpstreamError(format!(
            "{label} status={status} snippet={}",
            truncate(&body, 200)
        ))
        .into());
    }
    serde_json::from_str::<T>(&body).map_err(|e| {
        SjtuCliError::UpstreamError(format!(
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
