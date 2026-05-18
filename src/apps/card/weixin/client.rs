//! 注入主 jaccount session cookie 的 reqwest Client。
//!
//! Cookie struct (src/cookies/mod.rs:24-33) 是纯数据无方法，故本地手卷
//! `cookie_to_set_str` 拼成 `Set-Cookie` 形式喂 `reqwest::cookie::Jar::add_cookie_str`。
//!
//! `UA`、`cookie_to_set_str`、`build_weixin_client` 均为 `pub(super)`，
//! 供 Task 8（weixin/mod.rs 顶层 fetch_* 入口）使用；Task 8 尚未实现，故暂时 allow dead_code。

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::cookie::Jar;
use reqwest::redirect::Policy;
use reqwest::Client;

use crate::cookies::{Cookie, Session};
use crate::error::SjtuCliError;

/// Chrome 124 UA，由 Task 8 fetch_* 函数使用。
#[allow(dead_code)]
pub(super) const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// 把 `Cookie` 拼成 `name=value; Domain=...; Path=...` 字符串。
/// expires 故意不拼：jar 不在乎过期，stale 由 `SubSessionStale` 信号驱动重 CAS。
#[allow(dead_code)]
pub(super) fn cookie_to_set_str(c: &Cookie) -> String {
    let mut s = format!("{}={}", c.name, c.value);
    if let Some(d) = &c.domain {
        s.push_str(&format!("; Domain={d}"));
    }
    if let Some(p) = &c.path {
        s.push_str(&format!("; Path={p}"));
    }
    s
}

/// 构造 weixin path 用的 reqwest Client。注入主 session jaccount cookie。
#[allow(dead_code)]
pub(super) fn build_weixin_client(main_session: &Session) -> Result<Client> {
    let jar = Arc::new(Jar::default());
    let url = reqwest::Url::parse("https://weixin.sjtu.edu.cn/")
        .map_err(|e| SjtuCliError::NetworkError(format!("解析 weixin URL：{e}")))?;
    for c in &main_session.cookies {
        jar.add_cookie_str(&cookie_to_set_str(c), &url);
    }
    Client::builder()
        .cookie_provider(jar.clone())
        .redirect(Policy::limited(10))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .gzip(true)
        .user_agent(UA)
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 weixin Client：{e}")).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDur, Utc};

    fn fake_cookie(name: &str, value: &str) -> Cookie {
        Cookie {
            name: name.into(),
            value: value.into(),
            domain: Some(".sjtu.edu.cn".into()),
            path: Some("/".into()),
            expires: None,
        }
    }

    #[test]
    fn cookie_to_set_str_full_fields() {
        let c = fake_cookie("JAAuthCookie", "abc123");
        let s = cookie_to_set_str(&c);
        assert!(s.contains("JAAuthCookie=abc123"));
        assert!(s.contains("Domain=.sjtu.edu.cn"));
        assert!(s.contains("Path=/"));
        assert!(!s.contains("Expires"), "expires 故意不拼: {s}");
    }

    #[test]
    fn cookie_to_set_str_minimal() {
        let c = Cookie {
            name: "K".into(),
            value: "V".into(),
            domain: None,
            path: None,
            expires: None,
        };
        let s = cookie_to_set_str(&c);
        assert_eq!(s, "K=V");
    }

    #[test]
    fn build_weixin_client_with_empty_session_works() {
        let now = Utc::now();
        let s = Session {
            cookies: vec![],
            captured_at: now,
            soft_expires_at: now + ChronoDur::days(30),
        };
        let r = build_weixin_client(&s);
        assert!(r.is_ok(), "空 session 也应能 build client：{r:?}");
    }

    #[test]
    fn build_weixin_client_with_one_cookie_works() {
        let now = Utc::now();
        let s = Session {
            cookies: vec![fake_cookie("JAAuthCookie", "abc")],
            captured_at: now,
            soft_expires_at: now + ChronoDur::days(30),
        };
        assert!(build_weixin_client(&s).is_ok());
    }
}
