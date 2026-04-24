//! auth::oauth2 单测：用 mockito 模拟 3xx 跳转链 + 超限行为。
//!
//! 真实水源的 OAuth2 链路靠 CP-1（`sjtu shuiyuan login-probe`）手动验证。

use std::sync::Arc;

use reqwest::cookie::Jar;
use reqwest::redirect::Policy;
use reqwest::{Client, StatusCode};

use super::follow::{follow_redirect_chain, FollowResult};

/// 手动 redirect 的 client，不预填 cookie。
///
/// `no_proxy()` 是必须的：reqwest 默认继承 `HTTP_PROXY` / `HTTPS_PROXY` 环境变量，
/// 本机装了 Clash/V2ray 类代理时，mockito 的 127.0.0.1 请求会被劫持到代理里，
/// mock 永远收不到请求，断言就会挂。
fn bare_client() -> Client {
    Client::builder()
        .cookie_provider(Arc::new(Jar::default()))
        .redirect(Policy::none())
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap()
}

#[tokio::test]
async fn oauth2_follow_collects_cookies_across_hops() {
    let mut server = mockito::Server::new_async().await;

    // 模拟 shuiyuan → auth/jaccount → oauth2/authorize → callback → shuiyuan 的简化 3 跳。
    let m1 = server
        .mock("GET", "/start")
        .with_status(302)
        .with_header("location", "/auth")
        .with_header("set-cookie", "_forum_session=abc; Path=/")
        .create_async()
        .await;
    let m2 = server
        .mock("GET", "/auth")
        .with_status(302)
        .with_header("location", "/end")
        .with_header("set-cookie", "JSESSIONID=mid; Path=/")
        .create_async()
        .await;
    let m3 = server
        .mock("GET", "/end")
        .with_status(200)
        .with_header("set-cookie", "_t=final_token_value; Path=/")
        .with_body("ok")
        .create_async()
        .await;

    let target = format!("{}/start", server.url());
    let fr: FollowResult = follow_redirect_chain(&bare_client(), &target)
        .await
        .unwrap();

    m1.assert_async().await;
    m2.assert_async().await;
    m3.assert_async().await;

    assert!(fr.final_url.ends_with("/end"));
    assert_eq!(fr.final_status, StatusCode::OK);
    let names: std::collections::HashSet<_> =
        fr.collected.values().map(|c| c.name.clone()).collect();
    assert!(names.contains("_forum_session"));
    assert!(names.contains("JSESSIONID"));
    assert!(names.contains("_t"));
}

#[tokio::test]
async fn oauth2_follow_errors_on_redirect_loop() {
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("GET", "/loop")
        .with_status(302)
        .with_header("location", "/loop")
        .expect_at_least(12)
        .create_async()
        .await;

    let target = format!("{}/loop", server.url());
    let res = follow_redirect_chain(&bare_client(), &target).await;
    assert!(res.is_err(), "应在 12 跳后报错");
    let msg = format!("{:#}", res.unwrap_err());
    assert!(
        msg.contains("超过") || msg.contains("12"),
        "错误信息应提示跳数超限，实际：{msg}"
    );
}

#[tokio::test]
async fn oauth2_follow_surfaces_non_2xx_final_status() {
    let mut server = mockito::Server::new_async().await;
    // 模拟 OAuth2 授权确认页返回 200 HTML（reqwest 不重定向、直接暴露给落点判定层）。
    let _m = server
        .mock("GET", "/oauth2/authorize")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html>please authorize</html>")
        .create_async()
        .await;

    let target = format!("{}/oauth2/authorize", server.url());
    let fr = follow_redirect_chain(&bare_client(), &target)
        .await
        .unwrap();
    assert_eq!(fr.final_status, StatusCode::OK);
    assert!(fr.final_url.contains("/oauth2/authorize"));
}
