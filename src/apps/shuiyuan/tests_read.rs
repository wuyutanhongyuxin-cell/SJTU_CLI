//! apps::shuiyuan 只读端点 + 公共 util 单测：
//! - throttle 间隔保证
//! - 8 个 fixture JSON 反序列化（latest / topic / current_user / PM / notifications / search / tolerance）
//!
//! HTTP 路径（URL、header、error 分支）靠 CP-2/CP-3/CP-M1/CP-M2 真打水源验证。
//! 写端点 mockito 测试见 `tests_write.rs`。

use super::api::PmFilter;
use super::models::{
    CurrentUserEnvelope, LatestEnvelope, Notifications, SearchResult, TopicDetail,
};
use super::throttle::{Throttle, MIN_INTERVAL};

#[tokio::test]
async fn throttle_enforces_minimum_interval() {
    let t = Throttle::new();
    // 首次调用不 sleep。
    t.wait().await;
    let t0 = std::time::Instant::now();
    // 第二次应 sleep 到 ≥ MIN_INTERVAL。
    t.wait().await;
    let elapsed = t0.elapsed();
    // 允许下界小幅抖动（Windows 的 tokio::time::sleep 精度约 ±15ms）。
    assert!(
        elapsed >= MIN_INTERVAL.saturating_sub(std::time::Duration::from_millis(15)),
        "节流间隔过短：{elapsed:?}，期望 >= {MIN_INTERVAL:?}"
    );
}

#[test]
fn parse_session_current_user() {
    let fixture = r#"{"current_user":{"id":9999,"username":"alice","name":"Alice Zhang","uploaded_avatar_id":1}}"#;
    let env: CurrentUserEnvelope = serde_json::from_str(fixture).unwrap();
    assert_eq!(env.current_user.id, 9999);
    assert_eq!(env.current_user.username, "alice");
    assert_eq!(env.current_user.name.as_deref(), Some("Alice Zhang"));
}

#[test]
fn parse_latest_topics() {
    // tags 用真实水源返回的 object 结构 [{id,name,slug}]。CP-2 实测确认是 object 不是 string。
    let fixture = r#"{
      "users":[{"id":1,"username":"alice","avatar_template":"/a.png"}],
      "primary_groups":[],
      "topic_list":{
        "can_create_topic":true,
        "more_topics_url":"/latest?page=1",
        "per_page":30,
        "topics":[
          {"id":123,"title":"hello","fancy_title":"hello","posts_count":5,"reply_count":4,"views":100,"like_count":3,"last_posted_at":"2026-04-23T00:00:00.000Z","excerpt":"...","tags":[{"id":634,"name":"日记","slug":""},{"id":716,"name":"第五人格","slug":""}]},
          {"id":124,"title":"world","posts_count":1}
        ]
      }
    }"#;
    let env: LatestEnvelope = serde_json::from_str(fixture).unwrap();
    assert_eq!(env.topic_list.per_page, 30);
    assert_eq!(env.topic_list.topics.len(), 2);
    assert_eq!(env.topic_list.topics[0].id, 123);
    // Vec<Value> 能接 object 结构的 tag。
    assert_eq!(env.topic_list.topics[0].tags.len(), 2);
    let t0 = env.topic_list.topics[0].tags[0]
        .as_object()
        .expect("tag 应为 object");
    assert_eq!(t0["id"].as_u64(), Some(634));
    assert_eq!(t0["name"].as_str(), Some("日记"));
    assert_eq!(
        env.topic_list.more_topics_url.as_deref(),
        Some("/latest?page=1")
    );
    // 非核心字段缺失不应报错。
    assert_eq!(env.topic_list.topics[1].tags.len(), 0);
}

#[test]
fn parse_topic_detail_with_post_stream() {
    let fixture = r#"{
      "id":999,
      "title":"t",
      "fancy_title":"t!",
      "posts_count":2,
      "views":50,
      "like_count":7,
      "tags":["a"],
      "post_stream":{
        "posts":[
          {"id":1,"post_number":1,"username":"u1","created_at":"2026-04-23T00:00:00.000Z","raw":"**bold**","cooked":"<p><strong>bold</strong></p>"},
          {"id":2,"post_number":2,"username":"u2","cooked":"<p>ok</p>"}
        ]
      }
    }"#;
    let d: TopicDetail = serde_json::from_str(fixture).unwrap();
    assert_eq!(d.id, 999);
    assert_eq!(d.posts_count, 2);
    assert_eq!(d.post_stream.posts.len(), 2);
    assert_eq!(d.post_stream.posts[0].raw.as_deref(), Some("**bold**"));
    assert_eq!(d.post_stream.posts[1].raw, None);
}

#[test]
fn pm_filter_path_segments_are_correct() {
    // Discourse URL 形式：/topics/{path_segment}/{username}.json。
    // inbox 无后缀，其它三种带 -sent/-unread/-new。改错后 URL 就会打错端点。
    assert_eq!(PmFilter::Inbox.path_segment(), "private-messages");
    assert_eq!(PmFilter::Sent.path_segment(), "private-messages-sent");
    assert_eq!(PmFilter::Unread.path_segment(), "private-messages-unread");
    assert_eq!(PmFilter::New.path_segment(), "private-messages-new");
}

#[test]
fn parse_pm_inbox_topic_list() {
    // 真机（target/pm_inbox.json）截取的最小形状：PM 复用 topic_list + TopicSummary，
    // 每条 topic 多一个 `archetype=private_message` 字段。TopicSummary 现有字段应能容纳。
    let fixture = r#"{
      "users":[{"id":1,"username":"sysop"}],
      "topic_list":{
        "per_page":30,
        "topics":[
          {"id":404691,"title":"遭到举报的帖子被管理人员移除","archetype":"private_message","posts_count":1,"views":2,"like_count":0,"last_posted_at":"2025-08-11T13:50:07.594Z"},
          {"id":182124,"title":"感谢与我们共度时光","archetype":"private_message","posts_count":1}
        ]
      }
    }"#;
    let env: LatestEnvelope = serde_json::from_str(fixture).unwrap();
    assert_eq!(env.topic_list.topics.len(), 2);
    assert_eq!(env.topic_list.topics[0].id, 404691);
    assert_eq!(
        env.topic_list.topics[0].title,
        "遭到举报的帖子被管理人员移除"
    );
    // archetype 是 PM 专属；TopicSummary 没列这个字段，但 serde 默认忽略未知字段（我们没加 deny_unknown_fields），
    // 断言能成功 parse 就够 —— 等某天需要在 UI 区分 PM 时再给 TopicSummary 加字段。
    assert_eq!(env.topic_list.per_page, 30);
}

#[test]
fn parse_notifications() {
    let fixture = r#"{
      "notifications":[
        {"id":1,"notification_type":1,"read":false,"created_at":"2026-04-23T00:00:00.000Z","topic_id":100,"fancy_title":"hi","slug":"hi"},
        {"id":2,"notification_type":5,"read":true}
      ]
    }"#;
    let n: Notifications = serde_json::from_str(fixture).unwrap();
    assert_eq!(n.notifications.len(), 2);
    assert!(!n.notifications[0].read);
    assert!(n.notifications[1].read);
    assert_eq!(n.notifications[0].topic_id, Some(100));
}

#[test]
fn parse_search_result() {
    let fixture = r#"{
      "topics":[{"id":1,"title":"hit","posts_count":3}],
      "posts":[{"id":10,"topic_id":1,"blurb":"context...","username":"bob"}],
      "users":[]
    }"#;
    let r: SearchResult = serde_json::from_str(fixture).unwrap();
    assert_eq!(r.topics.len(), 1);
    assert_eq!(r.posts.len(), 1);
    assert_eq!(r.posts[0].topic_id, 1);
    assert_eq!(r.posts[0].blurb.as_deref(), Some("context..."));
}

#[test]
fn parse_empty_latest_tolerates_missing_fields() {
    let fixture = r#"{"topic_list":{"topics":[]}}"#;
    let env: LatestEnvelope = serde_json::from_str(fixture).unwrap();
    assert_eq!(env.topic_list.topics.len(), 0);
    assert_eq!(env.topic_list.per_page, 0);
}
