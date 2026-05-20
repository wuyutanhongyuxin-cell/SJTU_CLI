//! Zimbra SOAP HTTP client + 发包封装。
//!
//! 与 library 同范式：jaccount 主 session cookie 注入 reqwest jar，
//! GET mail.sjtu.edu.cn/ 触发 SSO 跟链拿 ZM_AUTH_TOKEN + JSESSIONID。
//!
//! **关键差异**：SOAP envelope 必须显式带 `<authToken>{ZM_AUTH_TOKEN}</authToken>`，
//! 仅靠 cookie 不被 SOAP 接受（500 + service.AUTH_REQUIRED）。
//!
//! 不需要 `.no_proxy()`：mail.sjtu.edu.cn 是标准 443 HTTPS，系统代理 routing
//! 规则能匹配 *.sjtu.edu.cn 直连（区别于 library:8080）。

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::{CONTENT_TYPE, REFERER, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::{Client, Url};

use super::soap::is_auth_required_fault;
use super::throttle::Throttle;
use crate::cookies::{Cookie, Session};
use crate::error::SjtuCliError;

pub(super) const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
pub(super) const BASE: &str = "https://mail.sjtu.edu.cn";
pub(super) const SOAP_URL: &str = "https://mail.sjtu.edu.cn/service/soap";

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

/// 同时返回 Client + Jar Arc，让 client.rs 在 SSO 跟链后能 inspect jar 抽 ZM_AUTH_TOKEN。
pub(super) fn build_http_client(main_session: &Session) -> Result<(Client, Arc<Jar>)> {
    let jar = Arc::new(Jar::default());
    for c in &main_session.cookies {
        let Some(d) = &c.domain else { continue };
        let host = d.trim_start_matches('.');
        let Ok(url) = Url::parse(&format!("https://{host}/")) else {
            continue;
        };
        jar.add_cookie_str(&cookie_to_set_str(c), &url);
    }
    let client = Client::builder()
        .cookie_provider(jar.clone())
        .redirect(Policy::limited(15))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .gzip(true)
        .user_agent(UA)
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 mail HTTP client: {e}")))?;
    Ok((client, jar))
}

/// 从 jar 抽 ZM_AUTH_TOKEN cookie value。SSO 跟链完毕后调。
pub(super) fn extract_zm_auth_token(jar: &Arc<Jar>) -> Option<String> {
    let url = Url::parse("https://mail.sjtu.edu.cn/").ok()?;
    let hv = jar.cookies(&url)?;
    let raw = hv.to_str().ok()?.to_string();
    for kv in raw.split(';') {
        let kv = kv.trim();
        if let Some(v) = kv.strip_prefix("ZM_AUTH_TOKEN=") {
            return Some(v.to_string());
        }
    }
    None
}

/// POST SOAP envelope，返回原始 XML 字符串。失败时按 Fault Code 分类。
pub(super) async fn send_soap_request(
    http: &Client,
    throttle: &Throttle,
    soap_envelope: &str,
    label: &str,
) -> Result<String> {
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        throttle.wait().await;
        match send_once(http, soap_envelope, label).await {
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

async fn send_once(http: &Client, envelope: &str, label: &str) -> Result<String> {
    let resp = http
        .post(SOAP_URL)
        .header(CONTENT_TYPE, "text/xml; charset=utf-8")
        .header(USER_AGENT, UA)
        .header(REFERER, format!("{BASE}/zimbra/mail"))
        .body(envelope.to_string())
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("POST SOAP {label}: {}", chain(&e))))?;

    let final_url = resp.url().to_string();
    if final_url.contains("jaccount.sjtu.edu.cn/jaccount/jalogin")
        || final_url.contains("jaccount.sjtu.edu.cn/oauth2/authorize")
    {
        return Err(SjtuCliError::SessionExpired.into());
    }

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("读 SOAP body {label}: {e}")))?;

    if is_auth_required_fault(&body) {
        return Err(SjtuCliError::SessionExpired.into());
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
    fn extract_zm_token_from_jar() {
        let jar = Arc::new(Jar::default());
        let url = Url::parse("https://mail.sjtu.edu.cn/").unwrap();
        jar.add_cookie_str(
            "ZM_AUTH_TOKEN=abc123; Domain=mail.sjtu.edu.cn; Path=/",
            &url,
        );
        let token = extract_zm_auth_token(&jar);
        assert_eq!(token.as_deref(), Some("abc123"));
    }

    #[test]
    fn extract_returns_none_when_absent() {
        let jar = Arc::new(Jar::default());
        let url = Url::parse("https://mail.sjtu.edu.cn/").unwrap();
        jar.add_cookie_str("OTHER=x; Domain=mail.sjtu.edu.cn; Path=/", &url);
        assert!(extract_zm_auth_token(&jar).is_none());
    }
}
