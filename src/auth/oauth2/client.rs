//! OAuth2 跟链用的 reqwest Client：注入主 session 所有 SJTU 域 cookie + 手动跟 redirect。
//!
//! 与 `auth::cas::client::build_client` 逻辑几乎一致，重写一份是为了让两条通道各自演化
//! （OAuth2 停点语义与 CAS 不同，共享 client 后再差异化停点判断会留隐患）。

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::cookie::Jar;
use reqwest::redirect::Policy;
use reqwest::Client;
use url::Url;

use crate::cookies::Session;
use crate::error::SjtuCliError;

/// 构造 client + 预注入主 session 的所有 SJTU 域 cookie。
///
/// - `redirect::Policy::none()`：手动跟链，不让 reqwest 吞中间 Set-Cookie。
/// - `timeout = 30s`：与 CAS 通道对齐；OAuth2 链多一跳 JAccount 授权、体感一致。
pub(super) fn build_client(main: &Session) -> Result<(Client, Arc<Jar>)> {
    let jar = Arc::new(Jar::default());

    // JAAuthCookie 要让 jaccount 域带上 —— 走 OAuth2 authorize 时决定是否自动授权。
    if let Some(jaauth) = main.get("JAAuthCookie") {
        let jaccount_url: Url = "https://jaccount.sjtu.edu.cn/".parse().expect("常量 URL");
        jar.add_cookie_str(
            &format!("JAAuthCookie={jaauth}; Domain=.sjtu.edu.cn; Path=/"),
            &jaccount_url,
        );
    }

    // 主 session 其他 SJTU 域 cookie 也带上（保险）。
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
        .redirect(Policy::none())
        .timeout(Duration::from_secs(30))
        .gzip(true)
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 reqwest client 失败: {e}")))?;

    Ok((client, jar))
}
