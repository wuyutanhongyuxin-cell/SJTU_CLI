//! OAuth2 rookie 兜底：从本机 Chrome/Edge/Firefox 读 `shuiyuan.sjtu.edu.cn` 域的 `_t` cookie。
//!
//! 触发条件：跟链停在 `jaccount.sjtu.edu.cn/oauth2/authorize`（授权确认页）。
//! 前提：用户已在该浏览器登过水源一次完成 OAuth2 人工授权。

use anyhow::Result;
use chrono::{DateTime, Utc};
use tracing::{debug, info};

use crate::cookies::{save_sub_session, Cookie, Session};
use crate::error::SjtuCliError;

/// 依次尝试 chrome → edge → firefox，首个含 `_t` 的结果落盘并返回。
pub(super) fn rookie_fallback_shuiyuan(name: &str) -> Result<Session> {
    let domains = vec!["shuiyuan.sjtu.edu.cn".to_string()];

    if let Some(session) = try_browser_shuiyuan("chrome", rookie::chrome(Some(domains.clone())))? {
        save_sub_session(name, &session)?;
        return Ok(session);
    }
    if let Some(session) = try_browser_shuiyuan("edge", rookie::edge(Some(domains.clone())))? {
        save_sub_session(name, &session)?;
        return Ok(session);
    }
    if let Some(session) = try_browser_shuiyuan("firefox", rookie::firefox(Some(domains)))? {
        save_sub_session(name, &session)?;
        return Ok(session);
    }

    Err(SjtuCliError::OAuth2Failed(
        "未在本机 Chrome/Edge/Firefox 里找到水源 _t cookie。请先在浏览器登录一次 \
         https://shuiyuan.sjtu.edu.cn/ 完成 OAuth2 授权，再重试。"
            .into(),
    )
    .into())
}

fn try_browser_shuiyuan<E: std::fmt::Display>(
    browser: &str,
    result: std::result::Result<Vec<rookie::enums::Cookie>, E>,
) -> Result<Option<Session>> {
    match result {
        Ok(raw) if !raw.is_empty() => {
            let cookies: Vec<Cookie> = raw.iter().map(cookie_from_rookie).collect();
            if cookies.iter().any(|c| c.name == "_t") {
                info!(browser, count = cookies.len(), "rookie 兜底拿到水源 _t");
                return Ok(Some(Session::new(cookies)));
            }
            Ok(None)
        }
        Ok(_) => {
            debug!(browser, "rookie 无水源 cookie");
            Ok(None)
        }
        Err(e) => {
            debug!(browser, error = %e, "rookie 读取失败");
            Ok(None)
        }
    }
}

fn cookie_from_rookie(c: &rookie::enums::Cookie) -> Cookie {
    Cookie {
        name: c.name.clone(),
        value: c.value.clone(),
        domain: Some(c.domain.clone()),
        path: if c.path.is_empty() {
            None
        } else {
            Some(c.path.clone())
        },
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
