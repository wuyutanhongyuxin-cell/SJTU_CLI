//! 水源 Discourse 只读端点。
//!
//! 拆出来以守住 `api.rs` 的 200 行硬限。本文件只放读端点 impl，
//! Client struct 定义、Client::connect、写端点转发、urlencoding helper
//! 都仍在 `api.rs`。
//!
//! 端点规则：
//! - 常规读走 `fetch_json`（节流 + JSON 解析 + 错误快照）
//! - `/session/current.json` 例外，404 是合法"未登录"语义

use anyhow::Result;
use reqwest::header::{ACCEPT, REFERER, USER_AGENT};
use reqwest::StatusCode;
use tracing::debug;

use super::api::{urlencoding, Client, PmFilter, SearchScope, BASE, UA};
use super::http::fetch_json;
use super::models::{
    CurrentUser, CurrentUserEnvelope, LatestEnvelope, Notifications, SearchResult, TopicDetail,
    TopicList,
};
use crate::error::SjtuCliError;

impl Client {
    /// GET /session/current.json：已登录 200、未登录 404 → Ok(None)。
    ///
    /// 不走公共 `fetch_json`，因为 404 在这里是合法语义（需要独立处理）。
    pub async fn current_user(&self) -> Result<Option<CurrentUser>> {
        self.throttle.wait().await;
        let url = format!("{BASE}/session/current.json");
        let resp = self
            .http
            .get(&url)
            .header(ACCEPT, "application/json")
            .header(USER_AGENT, UA)
            .header(REFERER, BASE)
            .header("Discourse-Logged-In", "true")
            .send()
            .await
            .map_err(|e| SjtuCliError::NetworkError(format!("GET {url}: {e}")))?;
        let status = resp.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            let snippet = resp.text().await.unwrap_or_default();
            return Err(SjtuCliError::ShuiyuanApi(format!(
                "/session/current.json status={status} snippet={snippet}"
            ))
            .into());
        }
        let env: CurrentUserEnvelope = resp
            .json()
            .await
            .map_err(|e| SjtuCliError::ShuiyuanApi(format!("current_user JSON 解析失败: {e}")))?;
        debug!(id = env.current_user.id, username = %env.current_user.username, "current_user OK");
        Ok(Some(env.current_user))
    }

    /// GET /latest.json?page=N。
    pub async fn latest_topics(&self, page: u32, limit: u32) -> Result<TopicList> {
        let url = format!("{BASE}/latest.json?page={page}");
        let env: LatestEnvelope =
            fetch_json(&self.http, &self.throttle, &url, "/latest.json").await?;
        let mut list = env.topic_list;
        if limit > 0 && list.topics.len() > limit as usize {
            list.topics.truncate(limit as usize);
        }
        Ok(list)
    }

    /// GET /t/{id}.json。
    pub async fn topic(&self, id: u64, post_limit: u32) -> Result<TopicDetail> {
        let url = format!("{BASE}/t/{id}.json?include_raw=1");
        let mut detail: TopicDetail =
            fetch_json(&self.http, &self.throttle, &url, "/t/<id>.json").await?;
        if post_limit > 0 && detail.post_stream.posts.len() > post_limit as usize {
            detail.post_stream.posts.truncate(post_limit as usize);
        }
        Ok(detail)
    }

    /// GET /notifications.json[?filter=unread]。
    pub async fn notifications(&self, unread_only: bool, limit: u32) -> Result<Notifications> {
        let url = if unread_only {
            format!("{BASE}/notifications.json?filter=unread")
        } else {
            format!("{BASE}/notifications.json")
        };
        let mut n: Notifications =
            fetch_json(&self.http, &self.throttle, &url, "/notifications.json").await?;
        if limit > 0 && n.notifications.len() > limit as usize {
            n.notifications.truncate(limit as usize);
        }
        Ok(n)
    }

    /// GET /topics/private-messages{-suffix}/{username}.json?page=N — 私信列表。
    ///
    /// Discourse 要求 URL 里带当前用户 username；此处内部先调 `/session/current.json` 取。
    /// 返回 `(username, TopicList)`，上层 handler 需要 username 放进 Envelope 供排查。
    pub async fn messages(
        &self,
        filter: PmFilter,
        page: u32,
        limit: u32,
    ) -> Result<(String, TopicList)> {
        let user = self
            .current_user()
            .await?
            .ok_or(SjtuCliError::NotAuthenticated)?;
        let username = user.username;
        let url = format!(
            "{BASE}/topics/{}/{}.json?page={}",
            filter.path_segment(),
            urlencoding(&username),
            page
        );
        let env: LatestEnvelope = fetch_json(
            &self.http,
            &self.throttle,
            &url,
            "/topics/private-messages*.json",
        )
        .await?;
        let mut list = env.topic_list;
        if limit > 0 && list.topics.len() > limit as usize {
            list.topics.truncate(limit as usize);
        }
        Ok((username, list))
    }

    /// GET /search.json?q=&type_filter=。
    pub async fn search(&self, q: &str, scope: SearchScope) -> Result<SearchResult> {
        let q_enc = urlencoding(q);
        let url = match scope {
            SearchScope::All => format!("{BASE}/search.json?q={q_enc}"),
            SearchScope::Topic => format!("{BASE}/search.json?q={q_enc}&type_filter=topic"),
            SearchScope::Post => format!("{BASE}/search.json?q={q_enc}&type_filter=post"),
        };
        fetch_json(&self.http, &self.throttle, &url, "/search.json").await
    }
}
