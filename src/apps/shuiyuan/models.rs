//! 水源 Discourse 响应结构体。非核心字段 `#[serde(default)]`/`Option<T>` 以抗 API 漂移。
//!
//! Step 4+ 按端点分批完善字段；此处先给 stub，让 Step 1 `cargo check` 通过。

use serde::{Deserialize, Serialize};

/// 当前用户（/session/current.json 里 `current_user` 字段）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUser {
    pub id: u64,
    pub username: String,
    #[serde(default)]
    pub name: Option<String>,
}

/// /session/current.json 顶层包装：`{"current_user": {...}}`
#[derive(Debug, Clone, Deserialize)]
pub(super) struct CurrentUserEnvelope {
    pub current_user: CurrentUser,
}

/// /latest.json 里 `topic_list` 的内容（我们拉平它作为对外模型）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopicList {
    #[serde(default)]
    pub topics: Vec<TopicSummary>,
    #[serde(default)]
    pub per_page: u32,
    #[serde(default)]
    pub more_topics_url: Option<String>,
}

/// /latest.json 顶层包装：`{"topic_list": {...}, "users": [...]}`，仅用于反序列化。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct LatestEnvelope {
    pub topic_list: TopicList,
}

/// /latest.json 里每条 topic 的摘要
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopicSummary {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub fancy_title: Option<String>,
    #[serde(default)]
    pub posts_count: u32,
    #[serde(default)]
    pub reply_count: u32,
    #[serde(default)]
    pub views: u32,
    #[serde(default)]
    pub like_count: u32,
    #[serde(default)]
    pub last_posted_at: Option<String>,
    #[serde(default)]
    pub excerpt: Option<String>,
    /// tags 用 Value 宽松解析：水源魔改 Discourse 可能返回 `[{"id":..,"name":..}]` 对象数组而非 `["a","b"]`
    #[serde(default)]
    pub tags: Vec<serde_json::Value>,
}

/// /t/{id}.json 顶层
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopicDetail {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub fancy_title: Option<String>,
    #[serde(default)]
    pub posts_count: u32,
    #[serde(default)]
    pub views: u32,
    #[serde(default)]
    pub like_count: u32,
    #[serde(default)]
    pub tags: Vec<serde_json::Value>,
    /// 发帖流：`post_stream.posts` 是楼层数组。
    #[serde(default)]
    pub post_stream: PostStream,
}

/// /t/{id}.json 里的 post_stream 子对象
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PostStream {
    #[serde(default)]
    pub posts: Vec<Post>,
}

/// 帖子楼层
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Post {
    pub id: u64,
    #[serde(default)]
    pub post_number: u32,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub created_at: Option<String>,
    /// 发帖者的原始 markdown。渲染时优先使用。
    #[serde(default)]
    pub raw: Option<String>,
    /// Discourse 渲染后的 HTML（raw 缺失时兜底）。
    #[serde(default)]
    pub cooked: Option<String>,
}

/// /notifications.json 顶层
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Notifications {
    #[serde(default)]
    pub notifications: Vec<Notification>,
}

/// 通知条目
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Notification {
    pub id: u64,
    #[serde(default)]
    pub notification_type: u32,
    #[serde(default)]
    pub read: bool,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub topic_id: Option<u64>,
    #[serde(default)]
    pub fancy_title: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
}

/// /search.json 顶层
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchResult {
    #[serde(default)]
    pub topics: Vec<TopicSummary>,
    #[serde(default)]
    pub posts: Vec<SearchPost>,
}

/// /search.json 里匹配到的帖子片段
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchPost {
    pub id: u64,
    #[serde(default)]
    pub topic_id: u64,
    #[serde(default)]
    pub blurb: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

/// GET /session/csrf.json → `{"csrf":"..."}`，写操作前强制拉一次。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct CsrfEnvelope {
    pub csrf: String,
}

/// POST /posts.json 成功响应（reply / new-topic 共用；new-topic 返回的是 1 楼）。
/// 字段取水源实测里稳定出现的；其余容忍缺失。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PostCreated {
    pub id: u64,
    #[serde(default)]
    pub post_number: u32,
    #[serde(default)]
    pub topic_id: u64,
    #[serde(default)]
    pub topic_slug: Option<String>,
    #[serde(default)]
    pub raw: Option<String>,
    #[serde(default)]
    pub cooked: Option<String>,
}

/// POST /post_actions 的响应宽松接：只判 2xx 算成功，返回原样 JSON 给上层透传。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LikeResult {
    /// 水源/Discourse 不同版本字段差异大，这里不枚举，直接透传。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
