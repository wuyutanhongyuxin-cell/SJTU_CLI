//! 水源 Discourse 客户端门面：struct 定义 + 连接 + 写端点转发 + 共享 helper。
//!
//! 读端点 impl 在 `api_read.rs`；写端点 impl 在 `api_write.rs`。
//! 拆分原因：守住本文件的 200 行硬限（CLAUDE.md）。

use std::sync::Arc;

use anyhow::Result;
use reqwest::Client as HttpClient;

use super::api_write;
use super::http::build_http_client;
use super::models::{LikeResult, PostCreated};
use super::throttle::Throttle;
use crate::auth::oauth2::oauth2_login;

pub(super) const START_URL: &str = "https://shuiyuan.sjtu.edu.cn/";
pub(super) const BASE: &str = "https://shuiyuan.sjtu.edu.cn";
pub(super) const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// 水源 Discourse 客户端。
///
/// 字段 `http` / `throttle` 以 `pub(super)` 暴露给 `api_read.rs` 的 `impl Client`，
/// 同时对 crate 外仍保持私有。
pub struct Client {
    pub(super) http: HttpClient,
    pub(super) throttle: Arc<Throttle>,
    /// OAuth2 返回的元数据（from_cache / elapsed_ms / via_rookie_fallback），给上层 Envelope 展示。
    pub login: OAuth2Meta,
}

/// 登录元数据，暴露给 CLI 命令构造 Envelope。
#[derive(Debug, Clone)]
pub struct OAuth2Meta {
    pub from_cache: bool,
    pub elapsed_ms: u128,
    pub via_rookie_fallback: bool,
    pub final_url: String,
}

/// 搜索范围：对应 Discourse `/search.json` 的 `type_filter`。
#[derive(Debug, Clone, Copy)]
pub enum SearchScope {
    All,
    Topic,
    Post,
}

/// 私信列表过滤器：对应 Discourse `/topics/private-messages{-suffix}/{username}.json`。
///
/// - `Inbox`：收件箱（默认，后缀为空）
/// - `Sent`：已发送
/// - `Unread`：未读
/// - `New`：新增
#[derive(Debug, Clone, Copy)]
pub enum PmFilter {
    Inbox,
    Sent,
    Unread,
    New,
}

impl PmFilter {
    /// URL 里的路径段：Discourse 把 inbox 以外的过滤器写成 `private-messages-xxx`。
    pub(super) fn path_segment(self) -> &'static str {
        match self {
            PmFilter::Inbox => "private-messages",
            PmFilter::Sent => "private-messages-sent",
            PmFilter::Unread => "private-messages-unread",
            PmFilter::New => "private-messages-new",
        }
    }
}

impl Client {
    /// 走 OAuth2 登录 → 注入 cookie → 构造带节流的 HTTP client。
    pub async fn connect() -> Result<Self> {
        let r = oauth2_login("shuiyuan", START_URL).await?;
        let http = build_http_client(&r.session)?;
        let login = OAuth2Meta {
            from_cache: r.from_cache,
            elapsed_ms: r.elapsed_ms,
            via_rookie_fallback: r.via_rookie_fallback,
            final_url: r.final_url,
        };
        Ok(Self {
            http,
            throttle: Arc::new(Throttle::new()),
            login,
        })
    }

    // —— 写端点转发（实现在 api_write.rs） ——

    /// POST /posts.json（reply）— 回复已有 topic。
    pub async fn reply(&self, topic_id: u64, raw: &str) -> Result<PostCreated> {
        api_write::reply(&self.http, &self.throttle, BASE, topic_id, raw).await
    }

    /// POST /posts.json（new-topic）— 发新帖。category 为 None 走默认分类。
    pub async fn new_topic(
        &self,
        category: Option<u64>,
        title: &str,
        raw: &str,
    ) -> Result<PostCreated> {
        api_write::new_topic(&self.http, &self.throttle, BASE, category, title, raw).await
    }

    /// POST /posts.json + archetype=private_message — 发私信给指定用户。
    pub async fn pm_send(&self, username: &str, title: &str, raw: &str) -> Result<PostCreated> {
        api_write::pm_send(&self.http, &self.throttle, BASE, username, title, raw).await
    }

    /// POST /post_actions — 点赞。
    pub async fn like(&self, post_id: u64) -> Result<LikeResult> {
        api_write::like(&self.http, &self.throttle, BASE, post_id).await
    }

    /// DELETE /t/{id}.json — 删整条 topic（不可恢复）。注意 PM 不能用此端点，请用 `archive_pm`。
    pub async fn delete_topic(&self, topic_id: u64) -> Result<()> {
        api_write::delete_topic(&self.http, &self.throttle, BASE, topic_id).await
    }

    /// PUT /t/{id}/archive-message.json — 把 PM 归档（从 sent/inbox 移走，进 archive 视图）。
    pub async fn archive_pm(&self, topic_id: u64) -> Result<()> {
        api_write::archive_pm(&self.http, &self.throttle, BASE, topic_id).await
    }

    /// DELETE /posts/{id}.json — 删单楼。首楼请走 `delete_topic`。
    pub async fn delete_post(&self, post_id: u64) -> Result<()> {
        api_write::delete_post(&self.http, &self.throttle, BASE, post_id).await
    }
}

/// 最小 URL encoding：水源 /search.json?q= 常见中文 / 空格 / #，无需额外 crate。
/// `pub(super)` 暴露给 api_write.rs / api_read.rs 复用。
pub(super) fn urlencoding(s: &str) -> String {
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
