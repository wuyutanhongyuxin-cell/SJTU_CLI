//! 手动跟 302 链、逐跳收 Set-Cookie。
//!
//! 返回 `(累积 cookie 表, 最终 URL, 最终 status)`。停点判定交给 `mod.rs::oauth2_login`，
//! 本模块只做透明的跟链 + cookie 收集，保持和 `cas::follow_redirect_chain` 一致的 HashMap 键语义。

use std::collections::HashMap;

use anyhow::Result;
use reqwest::header::{HeaderValue, LOCATION, USER_AGENT};
use reqwest::{Client, StatusCode};
use tracing::debug;
use url::Url;

use crate::cookies::Cookie;
use crate::error::SjtuCliError;

/// 跟 redirect 的最大跳数。OAuth2 链实测 5-7 跳；留 12 给未来演化。
const MAX_REDIRECT_HOPS: u8 = 12;

/// 模拟桌面浏览器 UA。裸 reqwest UA 被部分 SJTU SP 403。
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// cookie 累积表的 key：RFC 6265 §5.3 的 (name, domain, path) 三元组。
pub(super) type CookieKey = (String, String, String);

/// 手动跟链结果。
#[derive(Debug)]
pub(super) struct FollowResult {
    pub collected: HashMap<CookieKey, Cookie>,
    pub final_url: String,
    pub final_status: StatusCode,
}

/// 从 `start_url` 出发手动跟 302 链，逐跳收 Set-Cookie。
///
/// 退出条件：收到非 3xx 响应、或超过 `MAX_REDIRECT_HOPS` 跳。
pub(super) async fn follow_redirect_chain(
    client: &Client,
    start_url: &str,
) -> Result<FollowResult> {
    let mut url: Url = start_url
        .parse()
        .map_err(|e| SjtuCliError::InvalidInput(format!("非法 URL `{start_url}`: {e}")))?;

    let mut collected: HashMap<CookieKey, Cookie> = HashMap::new();

    for hop in 0..MAX_REDIRECT_HOPS {
        debug!(hop, url = %url, "OAuth2 hop");

        let resp = client
            .get(url.clone())
            .header(USER_AGENT, HeaderValue::from_static(UA))
            .send()
            .await
            .map_err(|e| SjtuCliError::NetworkError(format!("GET {url} 失败: {e}")))?;

        // 收本跳 Set-Cookie。
        // Set-Cookie 不带 Domain 属性 → host-only cookie → 域 = 当前请求的 host。
        // 直接填进 Cookie.domain 让下游重建 Client 时能正确识别归属。
        let source_host = url.host_str().unwrap_or("").to_string();
        for c in resp.cookies() {
            let domain_attr: Option<String> = c.domain().map(|s| s.to_string());
            let effective_domain = domain_attr.clone().unwrap_or_else(|| source_host.clone());
            let path = c.path().unwrap_or("").to_string();
            let key = (c.name().to_string(), effective_domain.clone(), path.clone());
            collected.insert(
                key,
                Cookie {
                    name: c.name().to_string(),
                    value: c.value().to_string(),
                    domain: Some(effective_domain),
                    path: if path.is_empty() { None } else { Some(path) },
                    expires: c.expires().map(|st| {
                        let dur = st.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                        chrono::DateTime::<chrono::Utc>::from_timestamp(dur.as_secs() as i64, 0)
                            .unwrap_or_else(chrono::Utc::now)
                    }),
                },
            );
        }

        let status = resp.status();
        if !is_redirect(status) {
            return Ok(FollowResult {
                collected,
                final_url: url.to_string(),
                final_status: status,
            });
        }

        let loc = resp
            .headers()
            .get(LOCATION)
            .ok_or_else(|| {
                SjtuCliError::UpstreamError(format!("{status} 重定向但缺少 Location 头"))
            })?
            .to_str()
            .map_err(|e| SjtuCliError::UpstreamError(format!("Location 头非 ASCII: {e}")))?
            .to_string();
        url = url
            .join(&loc)
            .map_err(|e| SjtuCliError::UpstreamError(format!("非法 Location `{loc}`: {e}")))?;
    }

    Err(SjtuCliError::UpstreamError(format!(
        "OAuth2 跟链超过 {MAX_REDIRECT_HOPS} 跳仍未落点（疑似死循环）"
    ))
    .into())
}

fn is_redirect(s: StatusCode) -> bool {
    matches!(
        s,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}
