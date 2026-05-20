//! 图书馆 HTTP Client 构造 + 公共 JSON GET 封装。
//!
//! 与 weixin/client.rs 同范式（主 jaccount session 直透传），但端点是 JSON XHR
//! 而非 HTML scrape，故 fetch_json 仍走 services 范式。
//!
//! **HTTP 8080 plain text**：weijieyue 后端不强 HTTPS，scheme 用 http://，端口 8080。
//! reqwest 默认接受 http scheme，无需 `.https_only(false)` 显式声明。
//!
//! 请求头硬约束（mimic 真机 chrome MCP 抓的）：
//! - `Accept: application/json, text/plain, */*`
//! - `Referer: <BASE>/wechat/sjtu/nowlend`
//! - **不**带 `X-Requested-With`（真机抓包未带；带上反而被 DWR 路由）

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::cookie::Jar;
use reqwest::header::{ACCEPT, REFERER, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::Client;

use super::throttle::Throttle;
use crate::cookies::{Cookie, Session};
use crate::error::SjtuCliError;

pub(super) const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
pub(super) const BASE: &str = "http://weijieyue.lib.sjtu.edu.cn:8080";

/// 把 `Cookie` 拼成 `Set-Cookie` 形式的字符串（供 `Jar::add_cookie_str`）。
fn cookie_to_set_str(c: &Cookie) -> String {
    let mut s = format!("{}={}", c.name, c.value);
    if let Some(d) = &c.domain {
        s.push_str(&format!("; Domain={d}"));
    }
    if let Some(p) = &c.path {
        s.push_str(&format!("; Path={p}"));
    }
    s
}

/// 注入 jaccount 主 session 的 reqwest Client。
///
/// 按每条 cookie 自身 `domain` 字段构造 base URL（trim 前导点），而非统一
/// 用 weijieyue URL；这与 weixin path L2 fix 同源 —— reqwest jar 按 RFC 6265
/// 严格 domain matching，统一 URL 会让 jaccount 域 cookie 被静默拒收。
pub(super) fn build_http_client(main_session: &Session) -> Result<Client> {
    let jar = Arc::new(Jar::default());
    for c in &main_session.cookies {
        let Some(d) = &c.domain else { continue };
        let host = d.trim_start_matches('.');
        // 用 https 兜底注入（cookie matching 不在乎 scheme，只看 domain + path），
        // 但实际 lib 域 cookie 也会一并 OK。
        let Ok(url) = reqwest::Url::parse(&format!("https://{host}/")) else {
            continue;
        };
        jar.add_cookie_str(&cookie_to_set_str(c), &url);
    }
    Client::builder()
        .cookie_provider(jar)
        .redirect(Policy::limited(15))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .gzip(true)
        .user_agent(UA)
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 library HTTP client: {e}")).into())
}

/// 公共 JSON GET：节流 + 标准 header + 连接层错重试 1 次 + 错误带 snippet。
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
    Err(last_err.expect("至少一次尝试错误"))
}

async fn fetch_once<T: serde::de::DeserializeOwned>(
    http: &Client,
    url: &str,
    label: &str,
) -> Result<T> {
    let resp = http
        .get(url)
        .header(ACCEPT, "application/json, text/plain, */*")
        .header(USER_AGENT, UA)
        .header(REFERER, format!("{BASE}/wechat/sjtu/nowlend"))
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("GET {url}: {}", chain(&e))))?;
    let final_url = resp.url().to_string();
    // 落地 URL 若在 jaccount 域，主 session 已失效。
    if final_url.contains("jaccount.sjtu.edu.cn/jaccount/jalogin")
        || final_url.contains("jaccount.sjtu.edu.cn/oauth2/authorize")
    {
        return Err(SjtuCliError::SessionExpired.into());
    }
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
    fn truncate_handles_utf8_multibyte_boundary() {
        let s = "你好世界abc";
        assert_eq!(truncate(s, 3), "你好世...");
    }

    #[test]
    fn cookie_to_set_str_full() {
        let c = Cookie {
            name: "JAAuthCookie".into(),
            value: "abc".into(),
            domain: Some(".sjtu.edu.cn".into()),
            path: Some("/".into()),
            expires: None,
        };
        let s = cookie_to_set_str(&c);
        assert!(s.contains("JAAuthCookie=abc"));
        assert!(s.contains("Domain=.sjtu.edu.cn"));
    }
}
