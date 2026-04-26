//! 水源 Discourse 写端点（reply / new_topic / pm_send / like / delete_topic / delete_post）。
//!
//! 安全边界：
//! - 写操作都必须带 `X-CSRF-Token`，token 来自 GET `/session/csrf.json`。
//! - 本模块只负责"把 HTTP 发出去"，是否询问用户、要不要 `--confirm` 由上层 handler 决定。
//! - 失败语义：4xx / 5xx → `SjtuCliError::ShuiyuanApi` 带 snippet；连接错 → `NetworkError`。
//!
//! HTTP 底层（CSRF / post /posts.json / finish）见 `api_write_http.rs`。

use anyhow::Result;
use reqwest::header::{ACCEPT, CONTENT_TYPE, REFERER, USER_AGENT};
use reqwest::Client as HttpClient;

use super::api::{urlencoding, UA};
use super::api_write_http::{csrf_token, finish, finish_empty, post_posts_json};
use super::models::{LikeResult, PostCreated};
use super::throttle::Throttle;
use crate::error::SjtuCliError;

/// POST /posts.json — 在已有 topic 下回复。
pub(super) async fn reply(
    http: &HttpClient,
    throttle: &Throttle,
    base: &str,
    topic_id: u64,
    raw: &str,
) -> Result<PostCreated> {
    let csrf = csrf_token(http, throttle, base).await?;
    let body = format!("raw={}&topic_id={}", urlencoding(raw), topic_id);
    post_posts_json(http, throttle, base, &csrf, body).await
}

/// POST /posts.json — 创建新 topic（category 可选，None 视为默认未归类）。
pub(super) async fn new_topic(
    http: &HttpClient,
    throttle: &Throttle,
    base: &str,
    category: Option<u64>,
    title: &str,
    raw: &str,
) -> Result<PostCreated> {
    let csrf = csrf_token(http, throttle, base).await?;
    let mut body = format!("raw={}&title={}", urlencoding(raw), urlencoding(title));
    if let Some(cid) = category {
        body.push_str(&format!("&category={cid}"));
    }
    post_posts_json(http, throttle, base, &csrf, body).await
}

/// POST /posts.json + archetype=private_message — 向指定用户发私信（新开 PM 会话）。
///
/// Discourse 约定：PM 是个 archetype=private_message 的 topic。
/// 水源魔改：收件人字段名是 `target_recipients`（标准 Discourse 用 `target_usernames`，
/// 在水源会被路由到一个 422 "您必须选择一个有效的用户。" 的死路径）。
/// 真机 CP-PM1（2026-04-26）实证：`target_usernames=百合师傅` → 422；
/// `target_recipients=百合师傅` → 200 创建成功。详见 tasks/lessons.md。
/// 字段值仍接受逗号分隔多人，目前 CLI 侧按单人收敛。
pub(super) async fn pm_send(
    http: &HttpClient,
    throttle: &Throttle,
    base: &str,
    username: &str,
    title: &str,
    raw: &str,
) -> Result<PostCreated> {
    let csrf = csrf_token(http, throttle, base).await?;
    let body = format!(
        "raw={}&title={}&archetype=private_message&target_recipients={}",
        urlencoding(raw),
        urlencoding(title),
        urlencoding(username),
    );
    post_posts_json(http, throttle, base, &csrf, body).await
}

/// POST /post_actions — 点赞已有楼层（post_action_type_id=2 = like）。
pub(super) async fn like(
    http: &HttpClient,
    throttle: &Throttle,
    base: &str,
    post_id: u64,
) -> Result<LikeResult> {
    let csrf = csrf_token(http, throttle, base).await?;
    throttle.wait().await;
    let url = format!("{base}/post_actions");
    let body = format!("id={post_id}&post_action_type_id=2&flag_topic=false");
    let resp = http
        .post(&url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, UA)
        .header(REFERER, base)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("X-Requested-With", "XMLHttpRequest")
        .header("X-CSRF-Token", &csrf)
        .body(body)
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("POST {url}: {e}")))?;
    finish::<LikeResult>(resp, "/post_actions").await
}

/// DELETE /t/{topic_id}.json — 删整条 topic（包括所有楼层，**不可恢复**）。
///
/// 水源魔改 Discourse 可能对普通用户 / 时间窗口做限制，落到 4xx 时 snippet 里带原因。
pub(super) async fn delete_topic(
    http: &HttpClient,
    throttle: &Throttle,
    base: &str,
    topic_id: u64,
) -> Result<()> {
    let csrf = csrf_token(http, throttle, base).await?;
    throttle.wait().await;
    let url = format!("{base}/t/{topic_id}.json");
    let resp = http
        .delete(&url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, UA)
        .header(REFERER, base)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("DELETE {url}: {e}")))?;
    finish_empty(resp, "/t/<id>.json DELETE").await
}

/// PUT /t/{topic_id}/archive-message.json — 把 PM 移入 archive 视图（从 inbox/sent 消失，仍可在 archive 找回）。
///
/// 水源 + 标准 Discourse 都对 PM 的 `DELETE /t/<id>.json` **静默 no-op**：返 200 + `deleted: true` 但 PM 仍在。
/// 真机 CP-PM1（2026-04-26）实证：DELETE 后 GET /t/<id>.json 仍 200 完整内容；改 PUT archive-message 后才从 sent 视图消失（returned=0）。
/// 详见 tasks/lessons.md "水源 PM 字段名 + 删除语义都魔改"。
pub(super) async fn archive_pm(
    http: &HttpClient,
    throttle: &Throttle,
    base: &str,
    topic_id: u64,
) -> Result<()> {
    let csrf = csrf_token(http, throttle, base).await?;
    throttle.wait().await;
    let url = format!("{base}/t/{topic_id}/archive-message.json");
    let resp = http
        .put(&url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, UA)
        .header(REFERER, base)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("PUT {url}: {e}")))?;
    finish_empty(resp, "/t/<id>/archive-message.json PUT").await
}

/// DELETE /posts/{post_id}.json — 删单楼（首楼 post 会被水源拒绝；请用 `delete_topic`）。
pub(super) async fn delete_post(
    http: &HttpClient,
    throttle: &Throttle,
    base: &str,
    post_id: u64,
) -> Result<()> {
    let csrf = csrf_token(http, throttle, base).await?;
    throttle.wait().await;
    let url = format!("{base}/posts/{post_id}.json");
    let resp = http
        .delete(&url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, UA)
        .header(REFERER, base)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("DELETE {url}: {e}")))?;
    finish_empty(resp, "/posts/<id>.json DELETE").await
}
