//! 注入主 jaccount session cookie 的 reqwest Client。
//!
//! Cookie struct (src/cookies/mod.rs:24-33) 是纯数据无方法，故本地手卷
//! `cookie_to_set_str` 拼成 `Set-Cookie` 形式喂 `reqwest::cookie::Jar::add_cookie_str`。
//!
//! `UA`、`cookie_to_set_str`、`build_weixin_client` 均为 `pub(super)`，
//! 供 weixin/mod.rs 顶层 `fetch_balance` / `fetch_history` / `fetch_history_summary` 使用。

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::cookie::Jar;
use reqwest::redirect::Policy;
use reqwest::Client;

use crate::cookies::{Cookie, Session};
use crate::error::SjtuCliError;

/// Chrome 124 UA，由 fetch_* 函数（weixin/mod.rs）间接使用。
pub(super) const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// 把 `Cookie` 拼成 `name=value; Domain=...; Path=...` 字符串。
/// expires 故意不拼：jar 不在乎过期，stale 由 `SubSessionStale` 信号驱动重 CAS。
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

/// 构造 weixin path 用的 reqwest Client。
///
/// **L2 fix**：按每个 cookie 自身 `domain` 字段构造 base URL（trim 前导点），
/// 而非用统一的 weixin URL。原写法把 jaccount 域 cookie 喂给 weixin URL，
/// reqwest jar 按 RFC 6265 严格 domain matching 静默拒收。
///
/// **L3 fix**：`Policy::none()`，自动 follow 改 `weixin_follow` 手卷
/// （绕开 reqwest 严格 URL parser 拒 OAuth2 scope 含裸空格 Location 的问题）。
pub(super) fn build_weixin_client(main_session: &Session) -> Result<Client> {
    let jar = Arc::new(Jar::default());
    for c in &main_session.cookies {
        let Some(d) = &c.domain else { continue };
        let host = d.trim_start_matches('.');
        let Ok(url) = reqwest::Url::parse(&format!("https://{host}/")) else {
            continue;
        };
        jar.add_cookie_str(&cookie_to_set_str(c), &url);
    }
    Client::builder()
        .cookie_provider(jar.clone())
        .redirect(Policy::none())
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

    #[test]
    fn build_weixin_client_accepts_jaccount_domain_cookie() {
        // L2 修复：jaccount 域 cookie 不该因 base URL=weixin 被静默拒绝。
        // 行为验证由 mockito 链路集成测试覆盖，本测试只确保不 panic + 接收多域 cookie。
        let now = Utc::now();
        let mut c = fake_cookie("JAAuthCookie", "abc");
        c.domain = Some("jaccount.sjtu.edu.cn".to_string());
        let s = Session {
            cookies: vec![c],
            captured_at: now,
            soft_expires_at: now + ChronoDur::days(30),
        };
        let _ = build_weixin_client(&s).expect("build OK");
    }
}
