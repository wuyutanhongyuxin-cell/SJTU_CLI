//! cas 单元测试：纯函数 + mockito 模拟跳转链。
//!
//! 不验真实 SJTU；真实子系统的 CAS 跳转在各自 app 的集成验证里跑（见 S3c/S3d/S3e 的 checkpoint）。

use std::sync::Arc;

use reqwest::cookie::Jar;
use reqwest::redirect::Policy;
use reqwest::{Client, StatusCode};

use super::*;

#[test]
fn detect_jaccount_host() {
    assert!(is_jaccount_host(
        "https://jaccount.sjtu.edu.cn/jaccount/login"
    ));
    assert!(!is_jaccount_host("https://my.sjtu.edu.cn/ui/app"));
    assert!(!is_jaccount_host("https://i.sjtu.edu.cn/xtgl"));
    assert!(!is_jaccount_host("not a url"));
}

#[test]
fn detect_redirect_status() {
    assert!(is_redirect(StatusCode::MOVED_PERMANENTLY));
    assert!(is_redirect(StatusCode::FOUND));
    assert!(is_redirect(StatusCode::SEE_OTHER));
    assert!(is_redirect(StatusCode::TEMPORARY_REDIRECT));
    assert!(is_redirect(StatusCode::PERMANENT_REDIRECT));
    assert!(!is_redirect(StatusCode::OK));
    assert!(!is_redirect(StatusCode::NOT_FOUND));
}

/// 给跟链测试用：构造手动 redirect 的 client，jar 不预填任何 cookie。
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
async fn follow_chain_collects_cookies_through_redirects() {
    let mut server = mockito::Server::new_async().await;

    // 模拟 SP → 中转 → SP 三跳，每跳设一个 cookie，最后 200。
    let m1 = server
        .mock("GET", "/start")
        .with_status(302)
        .with_header("location", "/middle")
        .with_header("set-cookie", "FIRST=v1; Path=/")
        .create_async()
        .await;
    let m2 = server
        .mock("GET", "/middle")
        .with_status(302)
        .with_header("location", "/end")
        .with_header("set-cookie", "MID=v2; Path=/")
        .create_async()
        .await;
    let m3 = server
        .mock("GET", "/end")
        .with_status(200)
        .with_header("set-cookie", "LAST=v3; Path=/")
        .with_body("ok")
        .create_async()
        .await;

    let target = format!("{}/start", server.url());
    let (cookies, final_url) = follow_redirect_chain(&bare_client(), &target)
        .await
        .unwrap();

    m1.assert_async().await;
    m2.assert_async().await;
    m3.assert_async().await;

    assert!(final_url.ends_with("/end"), "final_url={final_url}");
    let names: std::collections::HashSet<_> = cookies.values().map(|c| &c.name).collect();
    assert!(names.contains(&"FIRST".to_string()));
    assert!(names.contains(&"MID".to_string()));
    assert!(names.contains(&"LAST".to_string()));
}

#[tokio::test]
async fn follow_chain_errors_on_redirect_loop() {
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("GET", "/loop")
        .with_status(302)
        .with_header("location", "/loop")
        .expect_at_least(MAX_REDIRECT_HOPS as usize)
        .create_async()
        .await;

    let target = format!("{}/loop", server.url());
    let res = follow_redirect_chain(&bare_client(), &target).await;
    assert!(res.is_err(), "应在 {MAX_REDIRECT_HOPS} 跳后报错");
    let msg = format!("{:#}", res.unwrap_err());
    assert!(
        msg.contains("超过") || msg.contains(&MAX_REDIRECT_HOPS.to_string()),
        "错误信息应提示跳数超限，实际：{msg}"
    );
}

#[tokio::test]
async fn follow_chain_rejects_invalid_target_url() {
    let res = follow_redirect_chain(&bare_client(), "not://a valid url").await;
    assert!(res.is_err());
}

/// RFC 6265 §5.3：同 name 同 domain 不同 path 是两条独立 cookie，
/// 三元组 key 必须把它们分开存储，不能被 HashMap 覆盖成一条。
#[tokio::test]
async fn follow_chain_keeps_same_name_different_paths() {
    let mut server = mockito::Server::new_async().await;
    // mockito 的 with_header 在底层是 append，多次调用同一 header 会累加，
    // 真实模拟上游同时 Set-Cookie 两次（Path=/ 与 Path=/xtgl）的场景。
    let _m = server
        .mock("GET", "/multi")
        .with_status(200)
        .with_header("set-cookie", "JSESSIONID=root; Path=/")
        .with_header("set-cookie", "JSESSIONID=xtgl; Path=/xtgl")
        .with_body("ok")
        .create_async()
        .await;

    let target = format!("{}/multi", server.url());
    let (cookies, _) = follow_redirect_chain(&bare_client(), &target)
        .await
        .unwrap();

    let paths: std::collections::HashSet<_> = cookies
        .values()
        .filter(|c| c.name == "JSESSIONID")
        .map(|c| c.path.clone().unwrap_or_default())
        .collect();
    assert!(
        paths.contains("/"),
        "应保留 Path=/ 的 cookie，实际：{paths:?}"
    );
    assert!(
        paths.contains("/xtgl"),
        "应保留 Path=/xtgl 的 cookie，实际：{paths:?}"
    );
}
