//! 构造 jaccount.sjtu.edu.cn/oauth2/authorize URL + 用浏览器打开。
//!
//! state 生成：CSRF token 不要密码学强度，用 SystemTime nanos + pid 混淆够用。
//! 浏览器打开：复用 S1 已用的 headless_chrome（visible 模式），用户已 jAccount 登录则
//! 直接进入"授权 sjtu-cli 访问一卡通"页面，点同意即可。

use std::time::SystemTime;

use anyhow::{Context, Result};
use url::Url;

use crate::error::SjtuCliError;

pub const AUTHORIZE_URL: &str = "https://jaccount.sjtu.edu.cn/oauth2/authorize";
pub const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:45123/callback";
pub const DEFAULT_SCOPE: &str = "card_info card_transactions";

/// 构造 authorize URL：
/// `…/oauth2/authorize?response_type=code&client_id=…&redirect_uri=…&scope=…&state=…`
pub fn build_authorize_url(
    client_id: &str,
    redirect_uri: &str,
    scope: &str,
    state: &str,
) -> Result<String> {
    let mut url = Url::parse(AUTHORIZE_URL).context("解析 authorize URL")?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", scope)
        .append_pair("state", state);
    Ok(url.to_string())
}

/// 生成 state（CSRF token，非密码学）。
pub fn generate_state() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    // 32 字符 hex，足够 CSRF 用
    format!(
        "{:032x}",
        nanos
            .wrapping_mul(0x9e37_79b9_7f4a_7c15_u128)
            .wrapping_add(pid)
    )
}

/// 用 headless_chrome（可见模式）打开 authorize URL。
///
/// 用户在 jAccount 已登录态下点"同意"，浏览器会 302 到 127.0.0.1:45123/callback；
/// CLI 的本地 listener (callback.rs) 接住并解析 code。
///
/// 如果 chrome 启动失败 / 无图形界面，spec R6 留 `--manual-auth` 兜底（phase-2）。
pub async fn open_in_browser(url: &str) -> Result<()> {
    let url_owned = url.to_string();
    tokio::task::spawn_blocking(move || -> Result<()> {
        use headless_chrome::{Browser, LaunchOptions};
        let options = LaunchOptions::default_builder()
            .headless(false)
            .build()
            .map_err(|e| SjtuCliError::CardOAuth(format!("chrome 启动配置: {e}")))?;
        let browser = Browser::new(options)
            .map_err(|e| SjtuCliError::CardOAuth(format!("chrome 启动失败: {e}")))?;
        let tab = browser
            .new_tab()
            .map_err(|e| SjtuCliError::CardOAuth(format!("chrome new_tab: {e}")))?;
        tab.navigate_to(&url_owned)
            .map_err(|e| SjtuCliError::CardOAuth(format!("chrome navigate: {e}")))?;
        // 不 wait_until_navigated：authorize 页面会 302 到 127.0.0.1，
        // 而 127.0.0.1 listener 在另一个 task 等接受。这里只负责"把 URL 打开"。
        // 浏览器窗口由用户手动关闭（或 callback 写完 HTML 后用户关）。
        // browser drop 在 spawn_blocking 退出时发生，但 chrome 进程独立。
        //
        // SAFETY: 故意泄露 Browser ownership，避免 Drop 实现杀掉 chrome 进程。
        // OAuth2 流程需要浏览器持续存在直到用户点同意 + 看到 200 OK 后才关。
        // chrome 进程会在 CLI 退出时由 OS 回收。
        std::mem::forget(browser);
        Ok(())
    })
    .await
    .map_err(|e| SjtuCliError::CardOAuth(format!("spawn_blocking join: {e}")))??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_url_contains_all_params() {
        let s = build_authorize_url(
            "test_id",
            "http://127.0.0.1:45123/callback",
            "card_info card_transactions",
            "STATE123",
        )
        .unwrap();
        assert!(s.starts_with("https://jaccount.sjtu.edu.cn/oauth2/authorize?"));
        assert!(s.contains("response_type=code"));
        assert!(s.contains("client_id=test_id"));
        // url crate 会把空格 url-encode 为 +
        assert!(
            s.contains("scope=card_info+card_transactions")
                || s.contains("scope=card_info%20card_transactions"),
            "actual: {s}"
        );
        assert!(s.contains("state=STATE123"));
        // redirect_uri 应被 percent-encoded
        assert!(s.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A45123%2Fcallback"));
    }

    #[test]
    fn state_is_32_hex_chars() {
        let s = generate_state();
        assert_eq!(s.len(), 32);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn states_differ_between_calls() {
        // SystemTime 每次拿都不同（除非时钟一致到 ns）；用 pid 兜底
        let s1 = generate_state();
        // 防止 nanos 相同：sleep 微秒
        std::thread::sleep(std::time::Duration::from_micros(10));
        let s2 = generate_state();
        assert_ne!(s1, s2, "两次 state 不应相同");
    }
}
