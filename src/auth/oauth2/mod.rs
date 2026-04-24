//! JAccount OAuth2 跟链通道。S3a 为水源社区（shuiyuan.sjtu.edu.cn）所用。
//!
//! 与 `auth::cas` 的核心区别：
//! - CAS 落点若停在 jaccount 域 → 直接报失败。
//! - OAuth2 若停在 `jaccount.sjtu.edu.cn/oauth2/authorize` 授权确认页 → 触发 rookie 兜底。
//!
//! 不共享 `cas::follow_redirect_chain` 是有意为之：OAuth2 的停点语义差异应显式在本模块内编码，
//! 避免未来改一边漏另一边（lessons.md：不要过度工程化）。

mod client;
mod follow;
mod rookie;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::time::Instant;

use anyhow::{Context, Result};
use tracing::{debug, info, warn};
use url::Url;

use crate::cookies::{load_session, load_sub_session, save_sub_session, Cookie, Session};
use crate::error::SjtuCliError;

use follow::CookieKey;

/// `oauth2_login` 的返回值，与 `cas::CasResult` 形状对齐方便 Envelope 消费。
#[derive(Debug)]
pub struct OAuth2Result {
    pub session: Session,
    pub from_cache: bool,
    pub elapsed_ms: u128,
    pub final_url: String,
    /// 标记本次是否由 rookie 兜底完成（用于给用户提示"下次浏览器授权一次可走正路"）。
    pub via_rookie_fallback: bool,
}

/// 主入口：拿 `name` 子系统的水源 session。
///
/// `start_url` 是水源站点的起始 URL，本阶段固定传 `https://shuiyuan.sjtu.edu.cn/`。
pub async fn oauth2_login(name: &str, start_url: &str) -> Result<OAuth2Result> {
    let start = Instant::now();

    // 1) 缓存命中且未过期 → 直接返回。
    if let Ok(sess) = load_sub_session(name) {
        if !sess.is_expired() {
            info!(name, "命中 OAuth2 sub_session 缓存");
            return Ok(OAuth2Result {
                session: sess,
                from_cache: true,
                elapsed_ms: start.elapsed().as_millis(),
                final_url: start_url.to_string(),
                via_rookie_fallback: false,
            });
        }
        debug!(name, "sub_session 已过期，重走 OAuth2");
    }

    // 2) 主 session 必须存在且含 JAAuthCookie，否则无法走 OAuth2 自动授权。
    let main = load_session().context("主 session 不存在或损坏，请先 `sjtu login`")?;
    if main.get("JAAuthCookie").is_none() {
        return Err(SjtuCliError::SessionExpired.into());
    }

    // 3) 构造 client + 跟链。
    let (client, _jar) = client::build_client(&main)?;
    let fr = follow::follow_redirect_chain(&client, start_url).await?;
    debug!(
        name,
        final_url = %fr.final_url,
        final_status = %fr.final_status,
        collected = fr.collected.len(),
        "OAuth2 跟链完成"
    );

    // 4) 落点判定。
    let (host, path) = host_and_path(&fr.final_url);

    match host.as_deref() {
        Some("shuiyuan.sjtu.edu.cn") => finalize_shuiyuan(name, start, fr),
        Some("jaccount.sjtu.edu.cn") if path.contains("/oauth2/authorize") => {
            warn!(name, %path, "OAuth2 停在授权确认页，尝试 rookie 兜底");
            let session = rookie::rookie_fallback_shuiyuan(name)?;
            Ok(OAuth2Result {
                session,
                from_cache: false,
                elapsed_ms: start.elapsed().as_millis(),
                final_url: fr.final_url,
                via_rookie_fallback: true,
            })
        }
        Some("jaccount.sjtu.edu.cn")
            if path.contains("/jaccount/jalogin") || path.contains("/jaccount/login") =>
        {
            Err(SjtuCliError::SessionExpired.into())
        }
        _ => Err(SjtuCliError::OAuth2Failed(format!(
            "OAuth2 落到非预期 URL：{}（status={}）",
            fr.final_url, fr.final_status
        ))
        .into()),
    }
}

fn finalize_shuiyuan(name: &str, start: Instant, fr: follow::FollowResult) -> Result<OAuth2Result> {
    if !has_cookie(&fr.collected, "_t") {
        return Err(SjtuCliError::OAuth2Failed(format!(
            "水源落点未带 _t cookie（final_url={}）。OAuth2 未真正完成。",
            fr.final_url
        ))
        .into());
    }
    let cookies: Vec<Cookie> = fr.collected.into_values().collect();
    let session = Session::new(cookies);
    save_sub_session(name, &session)?;
    info!(
        name,
        cookie_count = session.cookies.len(),
        elapsed_ms = start.elapsed().as_millis() as u64,
        "水源 sub_session 已落盘"
    );
    Ok(OAuth2Result {
        session,
        from_cache: false,
        elapsed_ms: start.elapsed().as_millis(),
        final_url: fr.final_url,
        via_rookie_fallback: false,
    })
}

fn has_cookie(collected: &HashMap<CookieKey, Cookie>, name: &str) -> bool {
    collected.keys().any(|(n, _, _)| n == name)
}

fn host_and_path(url: &str) -> (Option<String>, String) {
    match Url::parse(url) {
        Ok(u) => (u.host_str().map(|h| h.to_string()), u.path().to_string()),
        Err(_) => (None, String::new()),
    }
}
