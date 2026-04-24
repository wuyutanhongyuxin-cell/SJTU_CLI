//! `sjtu shuiyuan <sub>` 的只读 handler：latest / topic / inbox / search / login-probe。
//! 写 handler 见 `handlers_write.rs`。

use anyhow::Result;

use crate::apps::shuiyuan::{to_plain, Client, Post};
use crate::output::{render, Envelope, OutputFormat};

use super::data::{
    InboxData, LatestData, LoginProbeData, MessagesData, RenderedPost, SearchData, TopicData,
};
use super::{PmFilterCli, RenderMode, SearchIn};

/// `sjtu shuiyuan latest`：拉 /latest.json?page=N 前 limit 条 topic。
pub async fn cmd_latest(page: u32, limit: u32, fmt: Option<OutputFormat>) -> Result<()> {
    let client = Client::connect().await?;
    let list = client.latest_topics(page, limit).await?;
    let data = LatestData {
        page,
        returned: list.topics.len(),
        per_page: list.per_page,
        more_topics_url: list.more_topics_url,
        topics: list.topics,
    };
    render(Envelope::ok(data), fmt)
}

/// `sjtu shuiyuan topic <id>`：拉 topic 详情与前 post_limit 楼。
pub async fn cmd_topic(
    id: u64,
    post_limit: u32,
    mode: RenderMode,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let client = Client::connect().await?;
    let detail = client.topic(id, post_limit).await?;
    let posts = detail
        .post_stream
        .posts
        .iter()
        .map(|p| render_post(p, mode))
        .collect();
    let data = TopicData {
        id: detail.id,
        title: detail.title,
        fancy_title: detail.fancy_title,
        posts_count: detail.posts_count,
        views: detail.views,
        like_count: detail.like_count,
        tags: detail.tags,
        render_mode: mode.as_str(),
        posts,
    };
    render(Envelope::ok(data), fmt)
}

/// `sjtu shuiyuan messages [--filter=inbox|sent|unread|new] [--page N] [--limit N]`：拉私信列表。
///
/// URL：`/topics/{path_segment}/{username}.json?page=N`。username 内部先拉 `/session/current.json`。
pub async fn cmd_messages(
    filter: PmFilterCli,
    page: u32,
    limit: u32,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let client = Client::connect().await?;
    let (username, list) = client.messages(filter.into(), page, limit).await?;
    let data = MessagesData {
        filter: filter.as_str(),
        username,
        page,
        returned: list.topics.len(),
        per_page: list.per_page,
        more_topics_url: list.more_topics_url,
        topics: list.topics,
    };
    render(Envelope::ok(data), fmt)
}

/// `sjtu shuiyuan inbox [--unread-only] [--limit]`：拉通知列表。
pub async fn cmd_inbox(unread_only: bool, limit: u32, fmt: Option<OutputFormat>) -> Result<()> {
    let client = Client::connect().await?;
    let n = client.notifications(unread_only, limit).await?;
    let data = InboxData {
        unread_only,
        returned: n.notifications.len(),
        notifications: n.notifications,
    };
    render(Envelope::ok(data), fmt)
}

/// `sjtu shuiyuan search <query> [--in]`：Discourse 全站搜索。
pub async fn cmd_search(query: String, scope: SearchIn, fmt: Option<OutputFormat>) -> Result<()> {
    let client = Client::connect().await?;
    let r = client.search(&query, scope.into()).await?;
    let data = SearchData {
        query,
        scope: scope.as_str(),
        topics_count: r.topics.len(),
        posts_count: r.posts.len(),
        topics: r.topics,
        posts: r.posts,
    };
    render(Envelope::ok(data), fmt)
}

/// `sjtu shuiyuan login-probe`：走 OAuth2 → GET /session/current.json 验证登录。
pub async fn cmd_login_probe(fmt: Option<OutputFormat>) -> Result<()> {
    let client = Client::connect().await?;
    let user = client.current_user().await?;
    let data = LoginProbeData {
        authenticated: user.is_some(),
        from_cache: client.login.from_cache,
        elapsed_ms: client.login.elapsed_ms,
        via_rookie_fallback: client.login.via_rookie_fallback,
        final_url: client.login.final_url.clone(),
        current_user: user,
    };
    render(Envelope::ok(data), fmt)
}

fn render_post(p: &Post, mode: RenderMode) -> RenderedPost {
    let body = match mode {
        RenderMode::Raw | RenderMode::Markdown => p.raw.clone().unwrap_or_default(),
        RenderMode::Plain => to_plain(p.raw.as_deref().unwrap_or("")),
    };
    RenderedPost {
        post_number: p.post_number,
        username: p.username.clone(),
        created_at: p.created_at.clone(),
        body,
    }
}
