//! 教务 HTTP Client 构造 + ZF 「GET-via-POST」公共封装（节流 + 重试 + 错误诊断）。
//!
//! cookie 注入：与 jwbmessage 同构，但目标 jar URL = `i.sjtu.edu.cn`。
//! ZF 后端 cookie 经 CAS 链可能落在 `jaccount.sjtu.edu.cn` / `i.sjtu.edu.cn` /
//! `sjtu.edu.cn` 多个子域，这里全收（后缀匹配 `*.sjtu.edu.cn`），reqwest jar 按 host 派发。
//!
//! 请求形态（§1.4）：
//! - `Content-Type: application/x-www-form-urlencoded;charset=UTF-8`
//! - `Accept: application/json, text/javascript, */*; q=0.01`
//! - `X-Requested-With: XMLHttpRequest`（缺会被路由到 HTML 兜底）
//! - `Origin: https://i.sjtu.edu.cn`
//! - `Referer: <page url with gnmkdm>`
//! - `User-Agent: <真实浏览器 UA>`（缺会触发 WAF）

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::cookie::Jar;
use reqwest::header::{ACCEPT, CONTENT_TYPE, ORIGIN, REFERER, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::Client;
use url::Url;

use super::throttle::Throttle;
use crate::cookies::Session;
use crate::error::SjtuCliError;

pub(super) const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
pub(super) const BASE: &str = "https://i.sjtu.edu.cn";
pub(super) const ACCEPT_JSON: &str = "application/json, text/javascript, */*; q=0.01";
pub(super) const CT_FORM: &str = "application/x-www-form-urlencoded;charset=UTF-8";

// 注意：SP 页面预 GET（`visit_sp_page`）拆到 `bind.rs` 守 200 行硬限。
// 调用方走 `super::bind::visit_sp_page`。

/// 把 `*.sjtu.edu.cn` 域 cookie 注入 reqwest jar。
pub(super) fn build_http_client(session: &Session) -> Result<Client> {
    let jar = Arc::new(Jar::default());
    let i_url: Url = "https://i.sjtu.edu.cn/".parse().expect("const URL");

    for c in &session.cookies {
        let domain = match c.domain.as_deref() {
            Some(d) if !d.is_empty() => d,
            _ => "i.sjtu.edu.cn",
        };
        if !domain.trim_start_matches('.').ends_with("sjtu.edu.cn") {
            continue;
        }
        let path = c.path.as_deref().unwrap_or("/");
        let s = format!("{}={}; Path={}", c.name, c.value, path);
        jar.add_cookie_str(&s, &i_url);
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

/// 公共 POST：节流 + ZF 标准 header + 连接层错自动重试 1 次 + 错误带 snippet。
///
/// `path`：URL path（如 `/cjcx/cjcx_cxXsgrcj.html`）。
/// `gnmkdm`：功能模块代码（如 `N305005`），会拼到 query 上。
/// `referer_path`：进入该 SP 时浏览器导航到的页面路径（提供 `Referer`，部分 SP 校验）。
/// `form`：`(name, value)` 列表，由调用方拼入 `queryModel.*` / `nd` / `time` 等。
pub(super) async fn post_form_json<T: serde::de::DeserializeOwned>(
    http: &Client,
    throttle: &Throttle,
    path: &str,
    gnmkdm: &str,
    referer_path: &str,
    form: &[(&str, String)],
    label: &str,
) -> Result<T> {
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        throttle.wait().await;
        match post_once(http, path, gnmkdm, referer_path, form, label).await {
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

async fn post_once<T: serde::de::DeserializeOwned>(
    http: &Client,
    path: &str,
    gnmkdm: &str,
    referer_path: &str,
    form: &[(&str, String)],
    label: &str,
) -> Result<T> {
    let url = format!("{BASE}{path}?doType=query&gnmkdm={gnmkdm}");
    let referer = format!("{BASE}{referer_path}?gnmkdm={gnmkdm}&layout=default");
    let resp = http
        .post(&url)
        .header(CONTENT_TYPE, CT_FORM)
        .header(ACCEPT, ACCEPT_JSON)
        .header(USER_AGENT, UA)
        .header(ORIGIN, BASE)
        .header(REFERER, &referer)
        .header("X-Requested-With", "XMLHttpRequest")
        .form(form)
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("POST {url}: {}", chain(&e))))?;

    let status = resp.status();
    let final_url = resp.url().to_string();
    let body = resp
        .text()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("{url}: 读 body: {e}")))?;

    // session 过期：CAS 把 final_url 改写到 jaccount，或正方主动给登录页 HTML
    if final_url.contains("jaccount.sjtu.edu.cn")
        || body.trim_start().starts_with("<!DOCTYPE")
        || body.trim_start().starts_with("<html")
    {
        return Err(SjtuCliError::UpstreamError(format!(
            "{label} session 已失效（被打回登录页 final_url={final_url}）；请 `sjtu logout` 后重新 `sjtu login`"
        )).into());
    }

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
