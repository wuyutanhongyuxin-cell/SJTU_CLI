//! LTI launch 的 headless Chrome 部分：navigate → 提 tokenId → 抓 v.sjtu cookie。
//!
//! 从 `auth.rs` 拆出，独立成模块原因：
//! - `auth.rs` 含外部 reqwest 调用 + 常量入口，逻辑层级与 chrome 驱动不同
//! - 文件行数硬限（200 行）—— 拆分后两文件均 < 200
//!
//! 本文件**全是同步代码**（headless_chrome crate 的 API 是同步），由 `auth.rs`
//! 在 `tokio::task::spawn_blocking` 内调用。

use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use headless_chrome::protocol::cdp::Network::{CookieParam, GetAllCookies};
use headless_chrome::{Browser, LaunchOptionsBuilder};
use tracing::warn;

use crate::cookies::Cookie;
use crate::error::SjtuCliError;

/// LTI launch URL 模板：oc.sjtu / courses / external_tools / <tool_id>?display=borderless。
const LTI_LAUNCH_FMT: &str =
    "https://oc.sjtu.edu.cn/courses/{course}/external_tools/{tool}?display=borderless";
/// 落地页 host 判定：URL 含此片段即认为 LTI launch 完成。
const LANDED_HOST: &str = "v.sjtu.edu.cn";
/// 落地页轮询超时（秒）：LTI 链 + JS 跳转一般 3-10s，给 30s 余量。
const LANDED_POLL_SECS: u64 = 30;
/// 轮询间隔。
const POLL_INTERVAL_MS: u64 = 250;

/// Chrome 跑出来的中间产物：tokenId + v.sjtu 域 cookie。
pub(super) struct ChromeLanded {
    pub token_id: String,
    pub v_cookies: Vec<Cookie>,
}

/// 同步执行 Chrome LTI launch。`auth::lti_launch` 在 spawn_blocking 内调本函数。
pub(super) fn run_chrome_launch(
    course_id: u64,
    lti_tool_id: u64,
    main_cookies: Vec<Cookie>,
) -> Result<ChromeLanded> {
    let opts = LaunchOptionsBuilder::default()
        .headless(true)
        .idle_browser_timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| anyhow!("构造 LaunchOptions 失败: {e}"))?;
    let browser = Browser::new(opts)
        .map_err(|e| SjtuCliError::UpstreamError(format!("Chrome 启动失败: {e}")))?;
    let tab = browser
        .new_tab()
        .map_err(|e| SjtuCliError::UpstreamError(format!("创建 tab 失败: {e}")))?;

    // 注入主 session cookie。
    // headless_chrome `set_cookies` 实测：若 url 字段空且当前 tab url 不是 http，会报错；
    // 因此为每条 cookie 显式给一个 https://<domain>/ 的 url 字段（详见 build_cookie_params）。
    let cookie_params = build_cookie_params(&main_cookies);
    if !cookie_params.is_empty() {
        tab.set_cookies(cookie_params)
            .map_err(|e| SjtuCliError::UpstreamError(format!("注入 cookie 失败: {e}")))?;
    } else {
        warn!("主 session 没有可用 cookie 注入到 Chrome");
    }

    // 导航到 LTI launch 起点。
    let launch_url = LTI_LAUNCH_FMT
        .replace("{course}", &course_id.to_string())
        .replace("{tool}", &lti_tool_id.to_string());
    tab.navigate_to(&launch_url)
        .map_err(|e| SjtuCliError::UpstreamError(format!("导航 LTI launch 失败: {e}")))?;
    let _ = tab.wait_until_navigated();

    // 轮询 URL，直到出现 v.sjtu 落地页。
    let token_id = wait_for_landed(&tab)?;

    // 抓 v.sjtu 域 cookie。
    let raw = tab
        .call_method(GetAllCookies(None))
        .map_err(|e| SjtuCliError::UpstreamError(format!("读浏览器 cookie 失败: {e}")))?
        .cookies;
    let v_cookies: Vec<Cookie> = raw
        .iter()
        .filter(|c| is_v_sjtu_domain(&c.domain))
        .map(cookie_from_cdp)
        .collect();

    Ok(ChromeLanded {
        token_id,
        v_cookies,
    })
}

/// 注入 Chrome 的 CDP CookieParam 转换；空 domain 回填 oc.sjtu.edu.cn（cas RFC 6265 §5.3 兜底）。
fn build_cookie_params(cookies: &[Cookie]) -> Vec<CookieParam> {
    cookies
        .iter()
        .filter_map(|c| {
            let domain = match c.domain.as_deref().unwrap_or("").trim_start_matches('.') {
                "" => "oc.sjtu.edu.cn",
                d => d,
            };
            if !domain.ends_with("sjtu.edu.cn") {
                return None;
            }
            Some(CookieParam {
                name: c.name.clone(),
                value: c.value.clone(),
                url: Some(format!("https://{domain}/")),
                domain: Some(domain.to_string()),
                path: c.path.clone().or_else(|| Some("/".to_string())),
                secure: Some(true),
                http_only: None,
                same_site: None,
                expires: None,
                priority: None,
                same_party: None,
                source_scheme: None,
                source_port: None,
                partition_key: None,
            })
        })
        .collect()
}

/// 轮询 tab.url 直到含 `LANDED_HOST` 且能提到 `tokenId`。
fn wait_for_landed(tab: &headless_chrome::Tab) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(LANDED_POLL_SECS);
    let mut last_url = String::new();
    while Instant::now() < deadline {
        let url = tab.get_url();
        last_url = url.clone();
        if url.contains(LANDED_HOST) {
            // 优先从 URL 提；提不到再用 evaluate_script 拿 location.href（hash 路由场景）。
            if let Some(t) = extract_token_id(&url) {
                return Ok(t);
            }
            if let Ok(remote) = tab.evaluate("location.href", false) {
                if let Some(href) = remote.value.and_then(|v| v.as_str().map(String::from)) {
                    if let Some(t) = extract_token_id(&href) {
                        return Ok(t);
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
    }
    Err(SjtuCliError::UpstreamError(format!(
        "LTI 落地超时（{LANDED_POLL_SECS}s 仍未拿到 tokenId；最后 URL：{}）",
        redact_url(&last_url),
    ))
    .into())
}

/// 把 URL 截到 host+path（去掉 query/fragment 的潜在 token），便于诊断卡点又不泄露。
fn redact_url(url: &str) -> String {
    if url.is_empty() {
        return "<empty>".to_string();
    }
    match url.find(['?', '#']) {
        Some(cut) => format!("{}?***", &url[..cut]),
        None => url.to_string(),
    }
}

/// 从形如 `https://v.sjtu.edu.cn/.../#/ivsModules/index?tokenId=XXX&...` 的 URL 提 tokenId。
///
/// 注意：tokenId 在 URL fragment 的 query 段里（`#/path?key=val`），不是顶级 query。
/// 不依赖 `url` crate 的 query_pairs（它不会解 fragment 内 query），手工查找。
pub(super) fn extract_token_id(url: &str) -> Option<String> {
    let needle = "tokenId=";
    let idx = url.find(needle)?;
    let tail = &url[idx + needle.len()..];
    let end = tail.find('&').unwrap_or(tail.len());
    let raw = &tail[..end];
    if raw.is_empty() {
        return None;
    }
    Some(raw.to_string())
}

/// 仅过 `*.v.sjtu.edu.cn` / `v.sjtu.edu.cn` 的 cookie。
fn is_v_sjtu_domain(domain: &str) -> bool {
    let d = domain.trim_start_matches('.');
    d == "v.sjtu.edu.cn" || d.ends_with(".v.sjtu.edu.cn")
}

fn cookie_from_cdp(c: &headless_chrome::protocol::cdp::Network::Cookie) -> Cookie {
    Cookie {
        name: c.name.clone(),
        value: c.value.clone(),
        domain: Some(c.domain.clone()),
        path: if c.path.is_empty() {
            None
        } else {
            Some(c.path.clone())
        },
        expires: None,
    }
}
