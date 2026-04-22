//! rookie 兜底：从本机 Chrome/Edge/Firefox 的 cookie 库直接读 SJTU 相关 cookie。
//!
//! 前提：用户已经在该浏览器里登过一次 JAccount（否则本机根本没 `JAAuthCookie`）。

use anyhow::Result;
use chrono::{DateTime, Utc};
use tracing::{debug, info};

use crate::cookies::{save_session, Cookie, Session};
use crate::error::SjtuCliError;

const DOMAINS: &[&str] = &["sjtu.edu.cn", "my.sjtu.edu.cn", "jaccount.sjtu.edu.cn"];

/// 依次尝试 Chrome / Edge / Firefox；取到第一个含 `JAAuthCookie` 的结果即用。
pub fn login_via_rookie() -> Result<Session> {
    let domains: Vec<String> = DOMAINS.iter().map(|s| s.to_string()).collect();

    if let Some(session) = try_browser("chrome", rookie::chrome(Some(domains.clone())))? {
        return Ok(session);
    }
    if let Some(session) = try_browser("edge", rookie::edge(Some(domains.clone())))? {
        return Ok(session);
    }
    if let Some(session) = try_browser("firefox", rookie::firefox(Some(domains.clone())))? {
        return Ok(session);
    }

    Err(SjtuCliError::UpstreamError(
        "未在本机 Chrome/Edge/Firefox 里找到 JAAuthCookie。请先在浏览器里登过 JAccount，或直接跑 `sjtu login` 走扫码流程。".into(),
    )
    .into())
}

fn try_browser<E: std::fmt::Display>(
    name: &str,
    result: std::result::Result<Vec<rookie::enums::Cookie>, E>,
) -> Result<Option<Session>> {
    match result {
        Ok(raw) if !raw.is_empty() => {
            debug!(browser = name, count = raw.len(), "rookie 拉到 cookie");
            let cookies: Vec<Cookie> = raw.iter().map(cookie_from_rookie).collect();
            if cookies.iter().any(|c| c.name == "JAAuthCookie") {
                info!(browser = name, "rookie 找到 JAAuthCookie，用它");
                let session = Session::new(cookies);
                save_session(&session)?;
                return Ok(Some(session));
            }
            Ok(None)
        }
        Ok(_) => {
            debug!(browser = name, "rookie 无相关 cookie");
            Ok(None)
        }
        Err(e) => {
            debug!(browser = name, error = %e, "rookie 读取失败");
            Ok(None)
        }
    }
}

fn cookie_from_rookie(c: &rookie::enums::Cookie) -> Cookie {
    Cookie {
        name: c.name.clone(),
        value: c.value.clone(),
        domain: Some(c.domain.clone()),
        expires: expires_to_datetime(c.expires),
    }
}

fn expires_to_datetime(expires: Option<u64>) -> Option<DateTime<Utc>> {
    let e = expires?;
    if e == 0 {
        return None;
    }
    DateTime::<Utc>::from_timestamp(e as i64, 0)
}
