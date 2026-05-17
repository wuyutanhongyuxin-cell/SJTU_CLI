//! mockito 单测：POST /oauth2/token 两种 grant + 两种错误路径。

use super::token::post_token_to;
use crate::error::SjtuCliError;

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        // no_proxy 防止本机代理（Clash / V2ray）劫持 mockito 127.0.0.1 请求
        .no_proxy()
        .build()
        .unwrap()
}

#[tokio::test]
async fn exchange_code_happy() {
    let mut server = mockito::Server::new_async().await;
    let m = server
        .mock("POST", "/oauth2/token")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"expires_in":1800,"token_type":"Bearer","refresh_token":"RFTOK","access_token":"ACCTOK"}"#,
        )
        .create_async()
        .await;
    let url = format!("{}/oauth2/token", server.url());
    let r = post_token_to(
        &http_client(),
        &url,
        &[
            ("grant_type", "authorization_code"),
            ("code", "CODE"),
            ("redirect_uri", "http://127.0.0.1:45123/callback"),
            ("client_id", "ID"),
            ("client_secret", "SECRET"),
        ],
        "exchange_code",
    )
    .await
    .expect("exchange_code 必须返回 TokenResponse");
    m.assert_async().await;
    assert_eq!(r.access_token, "ACCTOK");
    assert_eq!(r.refresh_token, "RFTOK");
    assert_eq!(r.expires_in, 1800);
    assert_eq!(r.token_type, "Bearer");
}

#[tokio::test]
async fn refresh_happy() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/oauth2/token")
        .with_status(200)
        .with_body(
            r#"{"expires_in":1800,"token_type":"Bearer","refresh_token":"NEW_RF","access_token":"NEW_AT"}"#,
        )
        .create_async()
        .await;
    let url = format!("{}/oauth2/token", server.url());
    let r = post_token_to(
        &http_client(),
        &url,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", "OLD_RF"),
            ("client_id", "ID"),
            ("client_secret", "SECRET"),
        ],
        "refresh",
    )
    .await
    .unwrap();
    assert_eq!(r.access_token, "NEW_AT");
    assert_eq!(r.refresh_token, "NEW_RF");
}

#[tokio::test]
async fn exchange_400_returns_card_oauth_err() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/oauth2/token")
        .with_status(400)
        .with_body(r#"{"error":"invalid_grant"}"#)
        .create_async()
        .await;
    let url = format!("{}/oauth2/token", server.url());
    let e = post_token_to(
        &http_client(),
        &url,
        &[("grant_type", "authorization_code")],
        "exchange_code",
    )
    .await
    .expect_err("400 应返回 Err");
    let downcasted = e.downcast_ref::<SjtuCliError>();
    assert!(matches!(downcasted, Some(SjtuCliError::CardOAuth(s)) if s.contains("status=400")));
}

#[tokio::test]
async fn malformed_json_returns_card_oauth_err() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/oauth2/token")
        .with_status(200)
        .with_body("not json")
        .create_async()
        .await;
    let url = format!("{}/oauth2/token", server.url());
    let e = post_token_to(
        &http_client(),
        &url,
        &[("grant_type", "refresh_token")],
        "refresh",
    )
    .await
    .expect_err("malformed JSON 应返回 Err");
    let s = format!("{e}");
    assert!(s.contains("解析 token JSON 失败"), "actual: {s}");
}
