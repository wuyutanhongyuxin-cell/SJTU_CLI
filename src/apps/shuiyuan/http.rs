//! 水源 HTTP Client 构造 + 公共 JSON 请求封装（节流 + 重试 + 错误诊断）。
//!
//! 选型说明：
//! - 只注入 `shuiyuan.sjtu.edu.cn` 域 cookie，丢弃 OAuth2 途中 jaccount 域的，避免 jar 污染。
//! - `.http1_only()` + `.pool_idle_timeout(0)` 是实战踩坑：否则 reqwest HTTP/2 + 连接池复用
//!   会让 `/latest.json` 等大响应超时挂起。**不能**加 `.no_proxy()` —— 中国用户普遍有本地代理
//!   (Clash/v2ray 等)，系统 DNS 可能解析不了 sjtu.edu.cn，需要走 `HTTPS_PROXY` 环境变量。
//! - 请求失败对 connect/timeout 错自动重试 1 次（固定 500ms sleep），抗首次连接偶发抖动。

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

const BASE: &str = "https://shuiyuan.sjtu.edu.cn";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

pub(super) fn build_http_client(session: &Session) -> Result<Client> {
    let jar = Arc::new(Jar::default());
    let shuiyuan_url: Url = "https://shuiyuan.sjtu.edu.cn/".parse().expect("常量 URL");

    for c in &session.cookies {
        let domain = match c.domain.as_deref() {
            Some(d) if !d.is_empty() => d,
            _ => "shuiyuan.sjtu.edu.cn",
        };
        if !domain
            .trim_start_matches('.')
            .ends_with("shuiyuan.sjtu.edu.cn")
        {
            continue;
        }
        let path = c.path.as_deref().unwrap_or("/");
        let s = format!("{}={}; Path={}", c.name, c.value, path);
        jar.add_cookie_str(&s, &shuiyuan_url);
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
///
/// 重试策略：只对"连接 / 发送层"的网络错重试（`operation timed out` / `error sending request`）；
/// 4xx / 5xx / JSON 解析错立即上抛 —— 这些语义错重试也解决不了。
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
    Err(last_err.expect("至少 1 次尝试的错误"))
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
        .header("Discourse-Logged-In", "true")
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
        return Err(SjtuCliError::ShuiyuanApi(format!(
            "{label} status={status} snippet={}",
            truncate_snippet(&body, 200)
        ))
        .into());
    }
    serde_json::from_str::<T>(&body).map_err(|e| {
        SjtuCliError::ShuiyuanApi(format!(
            "{label} JSON 解析失败: {e}. snippet={}",
            truncate_snippet(&body, 300)
        ))
        .into()
    })
}

/// 错误是否值得重试：连接 / 发送层的瞬时错重试；4xx/JSON 不重试。
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

fn truncate_snippet(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
