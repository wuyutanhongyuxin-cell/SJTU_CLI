//! apps::jwbmessage 写端点 mockito 单测。
//!
//! 只验 "客户端侧构造正确"：URL / header / body。不打真实交我办。
//! 真实 2xx / 4xx 分支靠 CP-M3 真机验证（该端点不可恢复，默认跳过）。

use std::sync::Arc;

use reqwest::cookie::Jar;
use reqwest::redirect::Policy;
use reqwest::Client as HttpClient;

use super::api_write;
use super::throttle::Throttle;

/// mockito 专用 HTTP client：手动拒掉系统代理 —— 参考 shuiyuan::tests_write 里的 lesson。
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
async fn read_all_posts_empty_json_with_correct_headers() {
    let mut server = mockito::Server::new_async().await;
    // 断言：body 必须是 `{}`；Content-Type: application/json；X-Requested-With 必须带。
    let m = server
        .mock("POST", "/api/jwbmessage/message/readall")
        .match_header("content-type", "application/json")
        .match_header("x-requested-with", "XMLHttpRequest")
        .match_header("accept", "application/json")
        .match_body(mockito::Matcher::Exact("{}".to_string()))
        .with_status(200)
        .with_body(r#"{"errno":0,"success":true,"message":"ok"}"#)
        .create_async()
        .await;

    let http = bare_client();
    let t = Throttle::new();
    let r = api_write::read_all(&http, &t, &server.url()).await.unwrap();
    m.assert_async().await;
    assert_eq!(r.errno, 0);
    assert!(r.success);
    assert_eq!(r.message.as_deref(), Some("ok"));
}

#[tokio::test]
async fn read_all_empty_body_is_treated_as_default_ok() {
    let mut server = mockito::Server::new_async().await;
    let m = server
        .mock("POST", "/api/jwbmessage/message/readall")
        .with_status(200)
        .with_body("")
        .create_async()
        .await;

    let http = bare_client();
    let t = Throttle::new();
    let r = api_write::read_all(&http, &t, &server.url())
        .await
        .expect("空 body 也应视为成功");
    m.assert_async().await;
    assert_eq!(r.errno, 0);
    assert!(!r.success);
}

#[tokio::test]
async fn read_all_4xx_surfaces_snippet_error() {
    let mut server = mockito::Server::new_async().await;
    let _bad = server
        .mock("POST", "/api/jwbmessage/message/readall")
        .with_status(401)
        .with_body(r#"{"errno":401,"message":"unauthorized"}"#)
        .create_async()
        .await;

    let http = bare_client();
    let t = Throttle::new();
    let err = api_write::read_all(&http, &t, &server.url())
        .await
        .expect_err("401 应报错");
    let msg = format!("{err:#}");
    assert!(msg.contains("status=401"), "msg={msg}");
    assert!(msg.contains("unauthorized"), "msg={msg}");
}
