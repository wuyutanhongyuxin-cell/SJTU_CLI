//! 课堂视频 HTTP Client：v.sjtu / live.sjtu 域 cookie 注入 + 公共 POST JSON 封装。
//!
//! 与 jwc / jwbmessage / canvas 的 http.rs 同构；差异点：
//! - `token: <Bootstrap.token>` 是必带 header（不是 Authorization Bearer）。
//! - 所有 POST 请求体走 `content-type: application/json` 或 `application/x-www-form-urlencoded`；
//!   本 CP-V1 仅用到 JSON。
//! - 错误统一走 `SjtuCliError::UpstreamError`。

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::cookie::Jar;
use reqwest::header::{ACCEPT, REFERER, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::Client;
use url::Url;

use super::throttle::Throttle;
use crate::cookies::Cookie;
use crate::error::SjtuCliError;

/// 默认 UA，与 qr_login / cas / jwbmessage 对齐（浏览器同源）。
pub(super) const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
/// 后端 base：v.sjtu 课堂视频 API 根。
pub(super) const BASE: &str = "https://v.sjtu.edu.cn";
/// SPA referer：服务端实测要带这个固定 referer 否则 403。
pub(super) const SPA_REFERER: &str = "https://v.sjtu.edu.cn/jy-application-canvas-sjtu-ui/";

/// 把 `*.sjtu.edu.cn` 域 cookie 注入 reqwest jar，构造一个固定 45s timeout 的 Client。
///
/// 入参 `cookies` 来自 `auth::lti_launch` 提取的 v.sjtu JSESSIONID + route，
/// 可叠加主 session 的 jaccount cookie（保险起见）。
pub(super) fn build_http_client(cookies: &[Cookie]) -> Result<Client> {
    let jar = Arc::new(Jar::default());
    let v_url: Url = "https://v.sjtu.edu.cn/".parse().expect("const URL");

    for c in cookies {
        let domain = match c.domain.as_deref() {
            Some(d) if !d.is_empty() => d,
            _ => "v.sjtu.edu.cn",
        };
        if !domain.trim_start_matches('.').ends_with("sjtu.edu.cn") {
            continue;
        }
        let path = c.path.as_deref().unwrap_or("/");
        let s = format!("{}={}; Path={}", c.name, c.value, path);
        jar.add_cookie_str(&s, &v_url);
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

/// 公共 POST JSON：节流 + 标准 header（含 `token:`）+ 重试 1 次 + 错误带 snippet。
///
/// `body` 已是序列化好的 JSON 字符串；`token` 为 `Bootstrap.token`（HS512）。
pub(super) async fn post_json<T: serde::de::DeserializeOwned>(
    http: &Client,
    throttle: &Throttle,
    url: &str,
    token: &str,
    body: &str,
    label: &str,
) -> Result<T> {
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        throttle.wait().await;
        match post_once::<T>(http, url, token, body, label).await {
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

async fn post_once<T: serde::de::DeserializeOwned>(
    http: &Client,
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
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("POST {url}: {}", chain(&e))))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("{url}: 读 body: {e}")))?;
    if !status.is_success() {
        return Err(SjtuCliError::UpstreamError(format!(
            "{label} status={status} snippet={}",
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
