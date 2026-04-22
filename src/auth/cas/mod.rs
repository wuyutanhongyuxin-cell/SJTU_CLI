//! CAS 子系统跳转通用通道。手动跟 302 链，逐跳收 Set-Cookie，
//! 缓存到 `~/.sjtu-cli/sub_sessions/<name>.json`。
//! 不用 reqwest 默认 follow 的原因：自动跟会吞中间响应，拿不到 jaccount 链路上的 cookie；
//! 也无法在最终停在 jaccount（JAAuthCookie 过期 or 需要交互确认）时立刻报错。

use std::collections::HashMap;
use std::time::Instant;

use anyhow::{Context, Result};
use reqwest::header::{HeaderValue, LOCATION, USER_AGENT};
use reqwest::{Client, StatusCode};
use tracing::{debug, info, warn};
use url::Url;

use crate::cookies::{load_session, load_sub_session, save_sub_session, Cookie, Session};
use crate::error::SjtuCliError;

mod client;
#[cfg(test)]
mod tests;

use client::build_client;

/// 跟 redirect 的最大跳数。SJTU 实测一般 4-6 跳；给 10 留余量、防死循环。
const MAX_REDIRECT_HOPS: u8 = 10;
/// HTTP 超时（秒）。CAS 链路偶尔慢，但 30s 没动静基本是挂了。被 client.rs 用。
pub(super) const HTTP_TIMEOUT_SECS: u64 = 30;
/// 模拟桌面浏览器；交大某些 SP 对裸 reqwest UA 会 403。
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// `cas_login` 的返回值，附带是否命中缓存与耗时（给上层 Envelope 展示）。
#[derive(Debug)]
pub struct CasResult {
    pub session: Session,
    pub from_cache: bool,
    pub elapsed_ms: u128,
    pub final_url: String,
}

/// 主入口：拿 `name` 子系统的 session（命中缓存直接返回；否则发起 CAS 跳转 + 落盘）。
///
/// `target_url` 必须是该 SP 真正要进的 URL（如教务的 `https://i.sjtu.edu.cn/xtgl/index_initMenu.html`），
/// 不要给 SSO 中转域名。
pub async fn cas_login(name: &str, target_url: &str) -> Result<CasResult> {
    let start = Instant::now();

    // 1) 缓存命中且未过期 → 直接返回。
    if let Ok(sess) = load_sub_session(name) {
        if !sess.is_expired() {
            info!(name, "命中 sub_session 缓存");
            return Ok(CasResult {
                session: sess,
                from_cache: true,
                elapsed_ms: start.elapsed().as_millis(),
                final_url: target_url.to_string(),
            });
        }
        debug!(name, "sub_session 已过期，重做 CAS");
    }

    // 2) 主 session 必须存在，且含 JAAuthCookie。
    let main = load_session().context("主 session 不存在或损坏，请先 `sjtu login`")?;
    let jaauth = main
        .get("JAAuthCookie")
        .ok_or(SjtuCliError::SessionExpired)?
        .to_string();

    // 3) 构造 client：手动跟 redirect，cookie store 注入主 session 的 SJTU 域 cookie。
    let (client, _jar) = build_client(&main, &jaauth)?;

    // 4) 手动跟链，收每跳的 Set-Cookie。
    let (collected, final_url) = follow_redirect_chain(&client, target_url).await?;

    // 5) 验落点：必须跳出 jaccount 域；否则视为 CAS 失败。
    if is_jaccount_host(&final_url) {
        return Err(SjtuCliError::SubSystemUnreachable(
            "cas",
            format!(
                "CAS 跳转最终停在 jaccount 域（{final_url}）。可能 JAAuthCookie 过期，或该 SP 需要交互确认。请先 `sjtu logout && sjtu login`。"
            ),
        )
        .into());
    }

    if collected.is_empty() {
        warn!(name, "CAS 跳转完成但未抓到任何 Set-Cookie");
    }

    // 6) 落盘并返回。
    let cookies: Vec<Cookie> = collected.into_values().collect();
    let session = Session::new(cookies);
    save_sub_session(name, &session)?;

    info!(
        name,
        cookie_count = session.cookies.len(),
        elapsed_ms = start.elapsed().as_millis() as u64,
        "sub_session 已落盘"
    );

    Ok(CasResult {
        session,
        from_cache: false,
        elapsed_ms: start.elapsed().as_millis(),
        final_url,
    })
}

/// cookie 累积表的 key：RFC 6265 §5.3 的 (name, domain, path) 三元组。
/// 同名同域但不同 path 是两条独立 cookie，这里显式分开存。
pub(super) type CookieKey = (String, String, String);

/// 手动跟 302 链。返回 (累积 cookie, 最终 URL)。
async fn follow_redirect_chain(
    client: &Client,
    target_url: &str,
) -> Result<(HashMap<CookieKey, Cookie>, String)> {
    let mut url: Url = target_url
        .parse()
        .map_err(|e| SjtuCliError::InvalidInput(format!("非法 URL `{target_url}`: {e}")))?;

    let mut collected: HashMap<CookieKey, Cookie> = HashMap::new();

    for hop in 0..MAX_REDIRECT_HOPS {
        debug!(hop, url = %url, "CAS hop");

        let resp = client
            .get(url.clone())
            .header(USER_AGENT, HeaderValue::from_static(UA))
            .send()
            .await
            .map_err(|e| SjtuCliError::NetworkError(format!("GET {url} 失败: {e}")))?;

        // 收每跳的 Set-Cookie。
        for c in resp.cookies() {
            let domain = c.domain().unwrap_or("").to_string();
            let path = c.path().unwrap_or("").to_string();
            let key = (c.name().to_string(), domain.clone(), path.clone());
            collected.insert(
                key,
                Cookie {
                    name: c.name().to_string(),
                    value: c.value().to_string(),
                    domain: Some(domain),
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
            // 终点：拿到非 3xx 响应。
            return Ok((collected, url.to_string()));
        }

        // 跟 Location 头。
        let loc = resp
            .headers()
            .get(LOCATION)
            .ok_or_else(|| {
                SjtuCliError::UpstreamError(format!("{} 重定向但缺少 Location 头", status))
            })?
            .to_str()
            .map_err(|e| SjtuCliError::UpstreamError(format!("Location 头不是合法 ASCII: {e}")))?
            .to_string();
        url = url
            .join(&loc)
            .map_err(|e| SjtuCliError::UpstreamError(format!("非法 Location `{loc}`: {e}")))?;
    }

    Err(SjtuCliError::UpstreamError(format!(
        "CAS 跳转超过 {MAX_REDIRECT_HOPS} 跳仍未落点（疑似死循环）"
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

fn is_jaccount_host(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .map(|h| h == "jaccount.sjtu.edu.cn")
        .unwrap_or(false)
}
