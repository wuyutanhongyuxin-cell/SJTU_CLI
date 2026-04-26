//! apps::shuiyuan 写端点 mockito 单测 + confirm util 单测。
//!
//! 只验 "客户端侧构造正确"：URL / header / body。不打真实水源。
//! 真实 4xx 分支、CSRF 旋转、Discourse 魔改字段，靠真机 CP-W 验。
//!
//! 读端点 parse 测试见 `tests_read.rs`。

use std::sync::Arc;

use reqwest::cookie::Jar;
use reqwest::redirect::Policy;
use reqwest::Client as HttpClient;

use super::api_write;
use super::api_write_http;
use super::throttle::Throttle;

/// mockito 专用 HTTP client：手动拒掉系统代理（见 `src/auth/cas/tests.rs` 的 lesson）。
///
/// `.no_proxy()` 必须带——否则 reqwest 默认读 `HTTP_PROXY`/`HTTPS_PROXY`，
/// 本机开 Clash 时 mockito 的 127.0.0.1 请求会被拖到代理里，mock 永远收不到。
fn bare_client() -> HttpClient {
    HttpClient::builder()
        .cookie_provider(Arc::new(Jar::default()))
        .redirect(Policy::none())
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap()
}

#[tokio::test]
async fn csrf_token_ok_parses_body() {
    let mut server = mockito::Server::new_async().await;
    let m = server
        .mock("GET", "/session/csrf.json")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"csrf":"test-csrf-value"}"#)
        .create_async()
        .await;

    let http = bare_client();
    let throttle = Throttle::new();
    let got = api_write_http::csrf_token(&http, &throttle, &server.url())
        .await
        .unwrap();
    m.assert_async().await;
    assert_eq!(got, "test-csrf-value");
}

#[tokio::test]
async fn reply_posts_json_with_csrf_header_and_form_body() {
    let mut server = mockito::Server::new_async().await;
    let csrf_mock = server
        .mock("GET", "/session/csrf.json")
        .with_status(200)
        .with_body(r#"{"csrf":"deadbeef"}"#)
        .create_async()
        .await;
    // 断言：POST /posts.json 必须带 X-CSRF-Token=deadbeef，且 body 包含 topic_id / raw 的 form 编码。
    let post_mock = server
        .mock("POST", "/posts.json")
        .match_header("x-csrf-token", "deadbeef")
        .match_header("content-type", "application/x-www-form-urlencoded")
        .match_body(mockito::Matcher::AllOf(vec![
            mockito::Matcher::Regex("topic_id=123".into()),
            mockito::Matcher::Regex("raw=hi".into()),
        ]))
        .with_status(200)
        .with_body(r#"{"id":42,"post_number":7,"topic_id":123,"topic_slug":"t","raw":"hi","cooked":"<p>hi</p>"}"#)
        .create_async()
        .await;

    let http = bare_client();
    let throttle = Throttle::new();
    let post = api_write::reply(&http, &throttle, &server.url(), 123, "hi")
        .await
        .unwrap();
    csrf_mock.assert_async().await;
    post_mock.assert_async().await;
    assert_eq!(post.id, 42);
    assert_eq!(post.post_number, 7);
    assert_eq!(post.topic_id, 123);
}

#[tokio::test]
async fn reply_4xx_surfaces_snippet_error() {
    let mut server = mockito::Server::new_async().await;
    let _csrf = server
        .mock("GET", "/session/csrf.json")
        .with_status(200)
        .with_body(r#"{"csrf":"x"}"#)
        .create_async()
        .await;
    let _bad = server
        .mock("POST", "/posts.json")
        .with_status(422)
        .with_body(r#"{"errors":["Title is too short"]}"#)
        .create_async()
        .await;

    let http = bare_client();
    let throttle = Throttle::new();
    let err = api_write::reply(&http, &throttle, &server.url(), 1, "x")
        .await
        .expect_err("422 应该报错");
    let msg = format!("{err:#}");
    assert!(msg.contains("status=422"), "msg={msg}");
    assert!(msg.contains("Title is too short"), "msg={msg}");
}

#[tokio::test]
async fn delete_topic_sends_csrf_header_and_hits_delete_endpoint() {
    let mut server = mockito::Server::new_async().await;
    let csrf_mock = server
        .mock("GET", "/session/csrf.json")
        .with_status(200)
        .with_body(r#"{"csrf":"del-token"}"#)
        .create_async()
        .await;
    // 断言：DELETE /t/468916.json 必须带 X-CSRF-Token=del-token。
    // 水源实测 body 常是空或 `{}`，这里返空 body 验证 finish_empty 不会因 JSON 解析挂掉。
    let del_mock = server
        .mock("DELETE", "/t/468916.json")
        .match_header("x-csrf-token", "del-token")
        .with_status(200)
        .with_body("")
        .create_async()
        .await;

    let http = bare_client();
    let throttle = Throttle::new();
    api_write::delete_topic(&http, &throttle, &server.url(), 468916)
        .await
        .expect("DELETE /t/<id>.json 空 body 也要算成功");
    csrf_mock.assert_async().await;
    del_mock.assert_async().await;
}

#[tokio::test]
async fn delete_post_4xx_surfaces_snippet_error() {
    let mut server = mockito::Server::new_async().await;
    let _csrf = server
        .mock("GET", "/session/csrf.json")
        .with_status(200)
        .with_body(r#"{"csrf":"x"}"#)
        .create_async()
        .await;
    let _bad = server
        .mock("DELETE", "/posts/42.json")
        .with_status(403)
        .with_body(r#"{"errors":["You are not permitted"]}"#)
        .create_async()
        .await;

    let http = bare_client();
    let throttle = Throttle::new();
    let err = api_write::delete_post(&http, &throttle, &server.url(), 42)
        .await
        .expect_err("403 应报错");
    let msg = format!("{err:#}");
    assert!(msg.contains("status=403"), "msg={msg}");
    assert!(msg.contains("not permitted"), "msg={msg}");
}

#[tokio::test]
async fn pm_send_posts_archetype_private_message_with_target() {
    let mut server = mockito::Server::new_async().await;
    let csrf_mock = server
        .mock("GET", "/session/csrf.json")
        .with_status(200)
        .with_body(r#"{"csrf":"pm-tok"}"#)
        .create_async()
        .await;
    // 断言 PM 关键 form 字段：archetype=private_message + target_recipients=<user> + title/raw。
    // 水源魔改：字段名是 target_recipients，不是标准 Discourse 的 target_usernames。
    let post_mock = server
        .mock("POST", "/posts.json")
        .match_header("x-csrf-token", "pm-tok")
        .match_header("content-type", "application/x-www-form-urlencoded")
        .match_body(mockito::Matcher::AllOf(vec![
            mockito::Matcher::Regex("archetype=private_message".into()),
            mockito::Matcher::Regex("target_recipients=alice".into()),
            mockito::Matcher::Regex("title=hi".into()),
            mockito::Matcher::Regex("raw=body".into()),
        ]))
        .with_status(200)
        .with_body(r#"{"id":7,"post_number":1,"topic_id":77,"raw":"body","cooked":"<p>body</p>"}"#)
        .create_async()
        .await;

    let http = bare_client();
    let throttle = Throttle::new();
    let post = api_write::pm_send(&http, &throttle, &server.url(), "alice", "hi", "body")
        .await
        .unwrap();
    csrf_mock.assert_async().await;
    post_mock.assert_async().await;
    assert_eq!(post.id, 7);
    assert_eq!(post.topic_id, 77);
}

#[tokio::test]
async fn archive_pm_puts_with_csrf_to_archive_message_endpoint() {
    let mut server = mockito::Server::new_async().await;
    let csrf_mock = server
        .mock("GET", "/session/csrf.json")
        .with_status(200)
        .with_body(r#"{"csrf":"arc-tok"}"#)
        .create_async()
        .await;
    // 断言：PUT /t/<id>/archive-message.json 必须带 X-CSRF-Token=arc-tok。
    // 水源真机实测 body 为空 / `{}`，这里返空 body 验 finish_empty 不会因 JSON 解析挂掉。
    let put_mock = server
        .mock("PUT", "/t/468916/archive-message.json")
        .match_header("x-csrf-token", "arc-tok")
        .with_status(200)
        .with_body("")
        .create_async()
        .await;

    let http = bare_client();
    let throttle = Throttle::new();
    api_write::archive_pm(&http, &throttle, &server.url(), 468916)
        .await
        .expect("PUT /t/<id>/archive-message.json 空 body 也要算成功");
    csrf_mock.assert_async().await;
    put_mock.assert_async().await;
}

#[tokio::test]
async fn archive_pm_4xx_surfaces_snippet_error() {
    let mut server = mockito::Server::new_async().await;
    let _csrf = server
        .mock("GET", "/session/csrf.json")
        .with_status(200)
        .with_body(r#"{"csrf":"x"}"#)
        .create_async()
        .await;
    let _bad = server
        .mock("PUT", "/t/1/archive-message.json")
        .with_status(404)
        .with_body(r#"{"errors":["topic not found"]}"#)
        .create_async()
        .await;

    let http = bare_client();
    let throttle = Throttle::new();
    let err = api_write::archive_pm(&http, &throttle, &server.url(), 1)
        .await
        .expect_err("404 应报错");
    let msg = format!("{err:#}");
    assert!(msg.contains("status=404"), "msg={msg}");
    assert!(msg.contains("topic not found"), "msg={msg}");
}

#[tokio::test]
async fn pm_send_4xx_surfaces_snippet_error() {
    let mut server = mockito::Server::new_async().await;
    let _csrf = server
        .mock("GET", "/session/csrf.json")
        .with_status(200)
        .with_body(r#"{"csrf":"x"}"#)
        .create_async()
        .await;
    let _bad = server
        .mock("POST", "/posts.json")
        .with_status(422)
        .with_body(r#"{"errors":["User can not receive messages"]}"#)
        .create_async()
        .await;

    let http = bare_client();
    let throttle = Throttle::new();
    let err = api_write::pm_send(&http, &throttle, &server.url(), "nobody", "t", "x")
        .await
        .expect_err("422 应报错");
    let msg = format!("{err:#}");
    assert!(msg.contains("status=422"), "msg={msg}");
    assert!(msg.contains("can not receive"), "msg={msg}");
}

#[test]
fn confirm_answer_no_aborts() {
    use crate::util::confirm::confirm_with_io;
    let mut out = Vec::<u8>::new();
    // assume_yes=true 会跳过 prompt——所以我们只测 assume_yes=true 场景下 "y" 走通 + "n" 走不到。
    // "非 TTY 硬失败" 分支: 在 Cargo test 里 stdin 本就非 TTY，传 assume_yes=false 必 Err。
    let err = confirm_with_io("测试动作", false, &mut out, || Ok("n\n".into()))
        .expect_err("非 TTY 且未 --yes 必须失败");
    let msg = format!("{err:#}");
    assert!(msg.contains("--yes") || msg.contains("非 TTY"), "msg={msg}");
}
