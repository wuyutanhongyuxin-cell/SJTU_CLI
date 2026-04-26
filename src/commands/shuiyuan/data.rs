//! `sjtu shuiyuan <sub>` 的数据形状。每个 `cmd_*` 对应一个 `*Data` 结构。
//!
//! 从 `handlers.rs` 拆出以守住 200 行硬限；这些类型仅在本模块内流转，
//! 外部通过 Envelope<T> 只看到序列化后的字段。

use serde::Serialize;

use crate::apps::shuiyuan::{
    CurrentUser, LikeResult, Notification, PostCreated, SearchPost, TopicSummary,
};

/// `login-probe` 命令的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct LoginProbeData {
    pub authenticated: bool,
    pub from_cache: bool,
    pub elapsed_ms: u128,
    pub via_rookie_fallback: bool,
    pub final_url: String,
    pub current_user: Option<CurrentUser>,
}

/// `latest` 命令的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct LatestData {
    pub page: u32,
    pub returned: usize,
    pub per_page: u32,
    pub more_topics_url: Option<String>,
    pub topics: Vec<TopicSummary>,
}

/// `topic` 命令的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct TopicData {
    pub id: u64,
    pub title: String,
    pub fancy_title: Option<String>,
    pub posts_count: u32,
    pub views: u32,
    pub like_count: u32,
    pub tags: Vec<serde_json::Value>,
    pub render_mode: &'static str,
    pub posts: Vec<RenderedPost>,
}

/// 单楼渲染后的形状。
#[derive(Debug, Serialize)]
pub(super) struct RenderedPost {
    pub post_number: u32,
    pub username: String,
    pub created_at: Option<String>,
    pub body: String,
}

/// `inbox` 命令的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct InboxData {
    pub unread_only: bool,
    pub returned: usize,
    pub notifications: Vec<Notification>,
}

/// `messages` 命令的 data 形状（私信列表）。
#[derive(Debug, Serialize)]
pub(super) struct MessagesData {
    pub filter: &'static str,
    pub username: String,
    pub page: u32,
    pub returned: usize,
    pub per_page: u32,
    pub more_topics_url: Option<String>,
    pub topics: Vec<TopicSummary>,
}

/// `search` 命令的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct SearchData {
    pub query: String,
    pub scope: &'static str,
    pub topics_count: usize,
    pub posts_count: usize,
    pub topics: Vec<TopicSummary>,
    pub posts: Vec<SearchPost>,
}

/// `reply` 命令的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct ReplyData {
    pub topic_id: u64,
    pub post: PostCreated,
}

/// `like` 命令的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct LikeData {
    pub post_id: u64,
    pub result: LikeResult,
}

/// `new-topic` 命令的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct NewTopicData {
    pub title: String,
    pub category: Option<u64>,
    pub post: PostCreated,
}

/// `pm-send` 命令的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct PmSendData {
    pub to: String,
    pub title: String,
    pub post: PostCreated,
}

/// `delete-topic` 命令的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct DeleteTopicData {
    pub topic_id: u64,
    pub deleted: bool,
}

/// `archive-pm` 命令的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct ArchivePmData {
    pub topic_id: u64,
    pub archived: bool,
}

/// `delete-post` 命令的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct DeletePostData {
    pub post_id: u64,
    pub deleted: bool,
}
