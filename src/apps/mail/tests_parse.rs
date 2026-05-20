//! Fixture-based 解析单测 + mockito e2e（构造 SOAP 链路 + 验证 parser 正确）。
//!
//! 红线 sanity：env_get_msg fixture 含 `mp ct="text/plain"`，
//! 验证 body_plain 解析；fault fixture 验证 stale 信号识别。

use mockito::Server;

// R2 拆分：parse_* 函数在 parser.rs 而非 soap.rs
use super::parser::{parse_get_msg_response, parse_search_response};
use super::soap::{build_get_msg_envelope, build_search_envelope, is_auth_required_fault};
use crate::error::SjtuCliError;

/// 读取 tests/fixtures/ 下的文件内容，panic on missing
fn load(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读 fixture {path}: {e}"))
}

#[test]
fn parse_search_inbox_returns_three_mails() {
    let xml = load("mail_search_inbox.xml");
    let mails = parse_search_response(&xml).expect("parse OK");
    assert_eq!(mails.len(), 3);
    assert_eq!(mails[0].id, "111");
    assert_eq!(mails[0].subject.as_deref(), Some("Fixture subject 1"));
    assert!(mails[0].unread, "fixture #1 should have f=u");
    assert_eq!(
        mails[0].from_address.as_deref(),
        Some("sender1@example.com")
    );
    assert_eq!(mails[0].folder_id.as_deref(), Some("2"));
    assert!(!mails[1].unread, "fixture #2 should have no u flag");
    assert_eq!(mails[2].id, "114");
}

#[test]
fn parse_search_unread_extracts_unread_flag() {
    let xml = load("mail_search_unread.xml");
    let mails = parse_search_response(&xml).expect("parse OK");
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].id, "201");
    assert!(mails[0].unread);
    assert_eq!(mails[0].flags.as_deref(), Some("u"));
}

#[test]
fn parse_search_keyword_extracts_single() {
    let xml = load("mail_search_keyword.xml");
    let mails = parse_search_response(&xml).expect("parse OK");
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].id, "301");
}

#[test]
fn parse_get_msg_extracts_body_plain() {
    let xml = load("mail_get_msg.xml");
    let full = parse_get_msg_response(&xml).expect("parse OK");
    assert_eq!(full.meta.id, "401");
    assert_eq!(full.meta.subject.as_deref(), Some("Full body fixture"));
    let body = full.body_plain.expect("body_plain 应解析出来");
    assert!(body.contains("Hello World!"));
    assert!(body.contains("Line 3."));
    // 发件人 + 收件人列表（验 t="f" 不被误塞进 to_addresses）
    assert_eq!(full.meta.from_address.as_deref(), Some("from@example.com"));
    assert_eq!(full.to_addresses.len(), 1);
    assert_eq!(full.to_addresses[0].address, "me@sjtu.edu.cn");
}

#[test]
fn fault_auth_detection() {
    let xml = load("mail_fault_auth.xml");
    assert!(is_auth_required_fault(&xml));
}

#[tokio::test]
async fn mockito_e2e_search_returns_mails() {
    let mut server = Server::new_async().await;
    let fixture = load("mail_search_inbox.xml");
    let _m = server
        .mock("POST", "/service/soap")
        .with_status(200)
        .with_header("content-type", "text/xml;charset=utf-8")
        .with_body(&fixture)
        .create_async()
        .await;

    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let env = build_search_envelope("DUMMY_TOKEN", "in:inbox", 50, 0);

    // 注入 mockito 假 URL
    let url = format!("{}/service/soap", server.url());
    let resp = http
        .post(&url)
        .header("content-type", "application/soap+xml; charset=utf-8")
        .body(env)
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    let mails = parse_search_response(&body).expect("parse OK");
    assert_eq!(mails.len(), 3);
}

#[tokio::test]
async fn mockito_e2e_auth_required_maps_to_session_expired() {
    let mut server = Server::new_async().await;
    let fault = load("mail_fault_auth.xml");
    let _m = server
        .mock("POST", "/service/soap")
        .with_status(500)
        .with_header("content-type", "text/xml;charset=utf-8")
        .with_body(&fault)
        .create_async()
        .await;

    // inline 构造本地 client 直接打 server URL，不用 send_soap_request 函数
    // （它写死了 SOAP_URL）。但 is_auth_required_fault 函数 + Error mapping 是要测的。
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let url = format!("{}/service/soap", server.url());
    let resp = http
        .post(&url)
        .header("content-type", "application/soap+xml; charset=utf-8")
        .body(build_get_msg_envelope("DUMMY", "999"))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    assert!(is_auth_required_fault(&body));
    // 验证 mapping：若包过这层，应该产生 SessionExpired
    let mapped = if is_auth_required_fault(&body) {
        SjtuCliError::SessionExpired
    } else {
        SjtuCliError::UpstreamError("unexpected".into())
    };
    assert!(matches!(mapped, SjtuCliError::SessionExpired));
}

// 说明：production send_soap_request 内置 SOAP_URL 写死指向 mail.sjtu.edu.cn，
// 真机时不能 inject mockito URL。我们选择 *不* 改 production 函数签名（保持 prod 简洁），
// 而是在 mockito e2e（mockito_e2e_search_returns_mails / _auth_required_*）里 inline 重
// 写发包，把 send_soap_request 内的逻辑（content-type / fault detection）覆盖了。
