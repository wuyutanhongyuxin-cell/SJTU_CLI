//! 水源社区（shuiyuan.sjtu.edu.cn） Discourse 客户端。
//!
//! 职责：
//! - 注入 OAuth2 跑下来的 `_t` / `_forum_session` cookie，构造带节流的 reqwest Client
//! - 封装 Discourse 只读端点：`/latest.json` / `/t/{id}.json` / `/notifications.json` / `/search.json` / `/session/current.json`
//! - 401/403 自动重走 `oauth2_login` 刷新（Step 4 实现）
//!
//! Step 3-6 填充；Step 1 先给骨架。

mod api;
mod api_read;
mod api_write;
mod api_write_http;
mod http;
mod models;
mod render;
#[cfg(test)]
mod tests_read;
#[cfg(test)]
mod tests_write;
mod throttle;

pub use api::{Client, OAuth2Meta, PmFilter, SearchScope};
pub use models::{
    CurrentUser, LikeResult, Notification, Notifications, Post, PostCreated, PostStream,
    SearchPost, SearchResult, TopicDetail, TopicList, TopicSummary,
};
pub use render::to_plain;
