//! 水源写端点共用 HTTP 层：CSRF 拉取、POST /posts.json 发送、响应 finish。
//!
//! 拆出来的目的：让 `api_write.rs` 只保留 6 个端点函数，守住 200 行硬限。
//! 本文件对 shuiyuan 模块内部 `pub(super)`；对 crate 外保持私有。

use anyhow::Result;
use reqwest::header::{ACCEPT, CONTENT_TYPE, REFERER, USER_AGENT};
use reqwest::Client as HttpClient;
use tracing::debug;

use super::api::UA;
use super::models::{CsrfEnvelope, PostCreated};
use super::throttle::Throttle;
use crate::error::SjtuCliError;

/// GET /session/csrf.json — 拉 CSRF token。写端点前必须调用。
pub(super) async fn csrf_token(
    http: &HttpClient,
    throttle: &Throttle,
    base: &str,
) -> Result<String> {
    throttle.wait().await;
    let url = format!("{base}/session/csrf.json");
    let resp = http
        .get(&url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, UA)
        .header(REFERER, base)
        .header("X-Requested-With", "XMLHttpRequest")
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("GET {url}: {e}")))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("{url}: 读 body: {e}")))?;
    if !status.is_success() {
        return Err(SjtuCliError::ShuiyuanApi(format!(
            "/session/csrf.json status={status} snippet={}",
            snippet(&body)
        ))
        .into());
    }
    let env: CsrfEnvelope = serde_json::from_str(&body).map_err(|e| {
        SjtuCliError::ShuiyuanApi(format!(
            "/session/csrf.json JSON 解析失败: {e}. snippet={}",
            snippet(&body)
        ))
    })?;
    debug!(len = env.csrf.len(), "csrf_token OK");
    Ok(env.csrf)
}

/// POST /posts.json — reply / new_topic / pm_send 共用的发送层。
pub(super) async fn post_posts_json(
    http: &HttpClient,
    throttle: &Throttle,
    base: &str,
    csrf: &str,
    body: String,
) -> Result<PostCreated> {
    throttle.wait().await;
    let url = format!("{base}/posts.json");
    let resp = http
        .post(&url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, UA)
        .header(REFERER, base)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("X-Requested-With", "XMLHttpRequest")
        .header("X-CSRF-Token", csrf)
        .body(body)
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("POST {url}: {e}")))?;
    finish::<PostCreated>(resp, "/posts.json").await
}

/// DELETE 端点共用的 2xx 判定：成功只看 status，不要求 body 解析。
///
/// Discourse 对 `/t/<id>.json` / `/posts/<id>.json` 的 DELETE 常见返回是空 body 或 `{}`，
/// 强制走 serde 反序列化会让正常路径报 "EOF while parsing"。
pub(super) async fn finish_empty(resp: reqwest::Response, label: &str) -> Result<()> {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(SjtuCliError::ShuiyuanApi(format!(
            "{label} status={status} snippet={}",
            snippet(&body)
        ))
        .into());
    }
    debug!(status = %status, "{label} OK");
    Ok(())
}

pub(super) async fn finish<T: serde::de::DeserializeOwned>(
    resp: reqwest::Response,
    label: &str,
) -> Result<T> {
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("{label}: 读 body: {e}")))?;
    if !status.is_success() {
        return Err(SjtuCliError::ShuiyuanApi(format!(
            "{label} status={status} snippet={}",
            snippet(&body)
        ))
        .into());
    }
    // 成功也兜一下 JSON 解析：水源若返回空体/HTML，明确报错比 silent 失败好。
    serde_json::from_str::<T>(&body).map_err(|e| {
        SjtuCliError::ShuiyuanApi(format!(
            "{label} JSON 解析失败: {e}. snippet={}",
            snippet(&body)
        ))
        .into()
    })
}

fn snippet(s: &str) -> String {
    const MAX: usize = 300;
    if s.len() <= MAX {
        s.to_string()
    } else {
        format!("{}...", &s[..MAX])
    }
}
