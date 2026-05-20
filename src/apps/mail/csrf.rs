//! Zimbra CSRF token 抽取。
//!
//! Zimbra 8+ 在 ZM_AUTH_TOKEN 里 set `csrf=1:1` flag 强制 envelope 携带
//! `<csrfToken>` element；token 嵌在 `/zimbra/mail` HTML body 里：
//!   `window.csrfToken = "0_<hex>";`
//!
//! 红线：仅 GET 只读 HTML body，纯字符串扫，不引 regex 依赖。

use anyhow::Result;
use reqwest::Client;

use super::http::{BASE, UA};
use crate::error::SjtuCliError;

/// 拉 `/zimbra/mail` HTML 抽 `window.csrfToken = "..."`。
///
/// SSO 跟链结束后调；返回 token raw 字符串（已去引号）。
pub(super) async fn fetch_csrf_token(http: &Client) -> Result<String> {
    let url = format!("{BASE}/zimbra/mail");
    let resp = http
        .get(&url)
        .header(reqwest::header::USER_AGENT, UA)
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("GET zimbra/mail (CSRF): {e}")))?;
    if !resp.status().is_success() {
        return Err(SjtuCliError::SubSystemUnreachable(
            "mail",
            format!("GET /zimbra/mail 返回 {}", resp.status()),
        )
        .into());
    }
    let body = resp
        .text()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("读 /zimbra/mail body: {e}")))?;
    parse_csrf(&body).ok_or_else(|| {
        SjtuCliError::SubSystemUnreachable(
            "mail",
            "未在 /zimbra/mail HTML 找到 window.csrfToken".into(),
        )
        .into()
    })
}

/// 纯字符串扫 `window.csrfToken = "<TOKEN>"`，返回 TOKEN。
fn parse_csrf(html: &str) -> Option<String> {
    let needle = "window.csrfToken";
    let idx = html.find(needle)?;
    let after = &html[idx + needle.len()..];
    let q1 = after.find('"')?;
    let rest = &after[q1 + 1..];
    let q2 = rest.find('"')?;
    Some(rest[..q2].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_csrf_from_typical_zimbra_html() {
        let html = r#"<html>
<script>
window.csrfToken            = "0_94a662e004948d2da4415a71d54a8559f9caa905";
localStorage.setItem("csrfToken" , "0_94a662e004948d2da4415a71d54a8559f9caa905");
</script>
</html>"#;
        assert_eq!(
            parse_csrf(html).as_deref(),
            Some("0_94a662e004948d2da4415a71d54a8559f9caa905")
        );
    }

    #[test]
    fn parse_csrf_returns_none_when_absent() {
        let html = "<html><body>no csrf here</body></html>";
        assert!(parse_csrf(html).is_none());
    }

    #[test]
    fn parse_csrf_handles_extra_whitespace_around_equals() {
        let html = r#"window.csrfToken="abc123";"#;
        assert_eq!(parse_csrf(html).as_deref(), Some("abc123"));
    }
}
