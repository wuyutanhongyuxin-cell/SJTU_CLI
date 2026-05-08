//! LTI 1.3 launch + tokenId → Bootstrap（HS512 token + courId + ltiCourseId）。
//!
//! 流程（详见 tasks/canvas_video_investigation.md §2.5 第 4 点）：
//! 1. `cas_login("canvas_oc", external_tools URL)`：让 oc.sjtu 域签发认证 session cookie。
//!    target 用 LTI launch URL —— 会 302→jaccount→oauth2_callback→external_tools form_post，
//!    前 4 跳给我们种好 oc 认证 cookie。用 `/login/canvas` 不行：返 200 HTML 静态页。
//!    name 用 `canvas_oc` 与 PAT 路径的 `canvas_token.txt` 区分。
//! 2. `auth_chrome::run_chrome_launch`：启 headless Chrome → 注入 oc 域 sub_session cookie →
//!    导航 oc.sjtu external_tools → 轮询拿 tokenId + 抓 v.sjtu cookie
//! 3. 本文件 `exchange_token`：reqwest 调 `getAccessTokenByTokenId` →
//!    `data.token` / `data.params.{courId,ltiCourseId}`
//! 4. 组装 `Bootstrap` 返回（**不持久化**：token TTL 1-3 小时）
//!
//! 行数硬限：本文件 + auth_chrome.rs 各 ≤ 200 行（CLAUDE.md §1）。

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::cookie::Jar;
use reqwest::header::{ACCEPT, REFERER, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::Client;
use tracing::{debug, info};
use url::Url;

use super::auth_chrome::{run_chrome_launch, ChromeLanded};
use super::http::{BASE, SPA_REFERER, UA};
use super::models::{Bootstrap, GetTokenResponse};
use crate::auth::cas::cas_login;
use crate::cookies::Cookie;
use crate::error::SjtuCliError;

/// CP-V1 主入口：跑一次完整 LTI launch，返回 Bootstrap。
///
/// 入参：
/// - `course_id`：Canvas 数字课程 ID（例 88168）。
/// - `lti_tool_id`：LTI 工具 ID（默认 8329 / "课堂视频new"）。
///
/// 主 session 由 `cas_login` 内部从磁盘读取（`load_session`）；未登录时 cas_login
/// 会抛"主 session 不存在或损坏，请先 `sjtu login`"。
///
/// 错误：
/// - `NotAuthenticated` / `SessionExpired` 由 `cas_login` 抛
/// - `UpstreamError` —— Chrome 启动失败 / 落地超时 / tokenId 缺失 / API 业务码非 0
pub async fn lti_launch(course_id: u64, lti_tool_id: u64) -> Result<Bootstrap> {
    // 1) cas_login 给 oc.sjtu 签认证 session（缓存 sub_sessions/canvas_oc.json）。
    //    target 用 LTI launch URL 触发完整 SSO 链；form_post 那跳停下时 cookie 已就绪。
    let cas_target = format!("https://oc.sjtu.edu.cn/courses/{course_id}/external_tools/{lti_tool_id}?display=borderless");
    let cas = cas_login("canvas_oc", &cas_target).await?;
    debug!(
        from_cache = cas.from_cache,
        cookie_count = cas.session.cookies.len(),
        elapsed_ms = cas.elapsed_ms as u64,
        "oc.sjtu sub_session 就绪"
    );

    // 2) Chrome 部分（阻塞 IO）走 spawn_blocking，避免阻塞 tokio reactor。
    let oc_cookies = cas.session.cookies.clone();
    let chrome_out: ChromeLanded =
        tokio::task::spawn_blocking(move || run_chrome_launch(course_id, lti_tool_id, oc_cookies))
            .await
            .map_err(|e| SjtuCliError::UpstreamError(format!("spawn_blocking 失败: {e}")))??;

    debug!(
        token_id_prefix = %redact_short(&chrome_out.token_id, 8),
        v_cookie_count = chrome_out.v_cookies.len(),
        "LTI 落地完成，开始换 token"
    );

    // 2) reqwest（注入 v.sjtu cookie）调 getAccessTokenByTokenId。
    let bootstrap = exchange_token(&chrome_out.token_id, &chrome_out.v_cookies).await?;

    info!(
        course_id,
        lti_tool_id,
        cour_id_prefix = %redact_short(&bootstrap.cour_id, 12),
        token_prefix = %redact_short(&bootstrap.token, 16),
        "Bootstrap 就绪（仅内存）"
    );

    Ok(bootstrap)
}

/// 用 reqwest（注入 v.sjtu cookie）调 `getAccessTokenByTokenId`，组装 Bootstrap。
async fn exchange_token(token_id: &str, v_cookies: &[Cookie]) -> Result<Bootstrap> {
    let client = build_token_client(v_cookies)?;
    let url = format!(
        "{BASE}/jy-application-canvas-sjtu/lti3/getAccessTokenByTokenId?tokenId={}",
        url_encode_component(token_id)
    );
    let resp = client
        .get(&url)
        .header(ACCEPT, "application/json, text/plain, */*")
        .header(USER_AGENT, UA)
        .header(REFERER, SPA_REFERER)
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("getAccessTokenByTokenId: {e}")))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("读 token body: {e}")))?;
    if !status.is_success() {
        return Err(SjtuCliError::UpstreamError(format!(
            "getAccessTokenByTokenId status={status} snippet={}",
            truncate(&text, 200)
        ))
        .into());
    }
    let parsed: GetTokenResponse = serde_json::from_str(&text).map_err(|e| {
        SjtuCliError::UpstreamError(format!(
            "getAccessTokenByTokenId JSON 解析失败: {e}. snippet={}",
            truncate(&text, 300)
        ))
    })?;
    if !parsed.is_business_ok() {
        return Err(SjtuCliError::UpstreamError(format!(
            "getAccessTokenByTokenId 业务失败 code={:?} msg={:?}",
            parsed.code, parsed.message
        ))
        .into());
    }
    extract_bootstrap(parsed, v_cookies)
}

/// 从 GetTokenResponse 取出三个必填字段，组装 Bootstrap。任一缺失即 UpstreamError。
fn extract_bootstrap(parsed: GetTokenResponse, v_cookies: &[Cookie]) -> Result<Bootstrap> {
    let data = parsed
        .data
        .ok_or_else(|| SjtuCliError::UpstreamError("token 响应缺 data".into()))?;
    let token = data
        .token
        .ok_or_else(|| SjtuCliError::UpstreamError("token 响应缺 data.token".into()))?;
    let params = data
        .params
        .ok_or_else(|| SjtuCliError::UpstreamError("token 响应缺 data.params".into()))?;
    let cour_id = params
        .cour_id
        .ok_or_else(|| SjtuCliError::UpstreamError("token 响应缺 data.params.courId".into()))?;
    let lti_course_id = params.lti_course_id.ok_or_else(|| {
        SjtuCliError::UpstreamError("token 响应缺 data.params.ltiCourseId".into())
    })?;
    Ok(Bootstrap {
        token,
        cour_id,
        lti_course_id,
        session_cookies: v_cookies.to_vec(),
    })
}

/// 构造一次性 reqwest client，专给 `getAccessTokenByTokenId` 调用使用。
fn build_token_client(v_cookies: &[Cookie]) -> Result<Client> {
    let jar = Arc::new(Jar::default());
    let v_url: Url = "https://v.sjtu.edu.cn/".parse().expect("const URL");
    for c in v_cookies {
        let path = c.path.as_deref().unwrap_or("/");
        jar.add_cookie_str(&format!("{}={}; Path={}", c.name, c.value, path), &v_url);
    }
    Client::builder()
        .cookie_provider(jar)
        .redirect(Policy::limited(5))
        .timeout(Duration::from_secs(30))
        .gzip(true)
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 token 换取 client 失败: {e}")).into())
}

/// `encodeURIComponent` 等价：与浏览器一致的 NON_ALPHANUMERIC 集（保留 `-_.~`）。
pub(super) fn url_encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

fn redact_short(s: &str, head: usize) -> String {
    if s.len() <= head {
        "***".into()
    } else {
        format!("{}***", &s[..head])
    }
}
