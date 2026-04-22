//! 把主 JAccount session 里的 cookie 注入到 reqwest 的 cookie jar，
//! 并构造一个"手动跟 redirect"的 Client。被 `cas_login` 复用。

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::cookie::Jar;
use reqwest::redirect::Policy;
use reqwest::Client;
use url::Url;

use super::HTTP_TIMEOUT_SECS;
use crate::cookies::Session;
use crate::error::SjtuCliError;

/// 构造 client + 预注入 SJTU 域所有 cookie。返回 jar 以便测试时可观察。
pub(super) fn build_client(main: &Session, jaauth: &str) -> Result<(Client, Arc<Jar>)> {
    let jar = Arc::new(Jar::default());

    // JAAuthCookie 必须能被 jaccount 域请求带上 —— CAS 跳到 jaccount 时它决定通行。
    let jaccount_url: Url = "https://jaccount.sjtu.edu.cn/".parse().expect("常量 URL");
    jar.add_cookie_str(
        &format!("JAAuthCookie={jaauth}; Domain=.sjtu.edu.cn; Path=/"),
        &jaccount_url,
    );

    // 主 session 里的其他 SJTU cookie 也带上（保险，如 my.sjtu 的辅助 cookie）。
    for c in &main.cookies {
        if c.name == "JAAuthCookie" {
            continue;
        }
        let domain = c.domain.as_deref().unwrap_or("sjtu.edu.cn");
        let cookie_str = format!("{}={}; Domain={}; Path=/", c.name, c.value, domain);
        let url_str = format!("https://{}/", domain.trim_start_matches('.'));
        if let Ok(u) = url_str.parse::<Url>() {
            jar.add_cookie_str(&cookie_str, &u);
        }
    }

    let client = Client::builder()
        .cookie_provider(jar.clone())
        .redirect(Policy::none()) // 关键：手动跟链才能逐跳收 Set-Cookie
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .gzip(true)
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 reqwest client 失败: {e}")))?;

    Ok((client, jar))
}
