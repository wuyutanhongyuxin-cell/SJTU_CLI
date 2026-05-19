//! `balance_parse` 兄弟测试文件（拆自 balance_parse.rs，避免单文件超 200 行硬限）。

#![cfg(test)]

use super::balance_parse::{extract_after_label, parse_balance};
use crate::apps::card::models::{CardFreezeStatus, CardLostStatus};
use rust_decimal::Decimal;

fn fixture() -> String {
    std::fs::read_to_string("tests/fixtures/card_balance_weixin.html").expect("读 fixture 失败")
}

#[test]
fn parses_complete_fixture() {
    let ci = parse_balance(&fixture()).unwrap();
    assert_eq!(ci.card_no, "123456");
    assert_eq!(ci.card_balance, Decimal::from_str_exact("3.88").unwrap());
    assert_eq!(ci.trans_balance, Decimal::ZERO);
    assert_eq!(ci.lost_status, Some(CardLostStatus::Normal));
    assert_eq!(ci.freeze_status, Some(CardFreezeStatus::Normal));
}

#[test]
fn pii_fields_not_in_card_info() {
    let ci = parse_balance(&fixture()).unwrap();
    assert!(ci.user.is_none(), "user 应保持 None（PII 不写入）");
    assert!(ci.bank_no.is_none(), "bank_no weixin path 应保持 None");
}

#[test]
fn missing_card_balance_caption_errors() {
    // table-condensed 存在但没 caption
    let html = r#"<table class="table table-condensed"><tbody><tr><td>卡账号：</td><td>X</td></tr></tbody></table>"#;
    let r = parse_balance(html);
    assert!(r.is_err());
    let msg = format!("{:#}", r.unwrap_err());
    assert!(msg.contains("校园卡余额"), "错误应提及字段：{msg}");
}

#[test]
fn missing_card_no_errors() {
    let html = r#"<table class="table table-condensed">
        <caption><strong>校园卡余额：1 元</strong></caption>
        <tbody></tbody>
    </table>"#;
    let r = parse_balance(html);
    assert!(r.is_err());
    let msg = format!("{:#}", r.unwrap_err());
    assert!(msg.contains("卡账号"), "错误应提及字段：{msg}");
}

#[test]
fn missing_table_class_errors() {
    let html = r#"<html><body><p>no table here</p></body></html>"#;
    let r = parse_balance(html);
    assert!(r.is_err());
    let msg = format!("{:#}", r.unwrap_err());
    assert!(msg.contains("table-condensed"), "错误应提及缺 table：{msg}");
}

#[test]
fn lost_status_lost_variant() {
    let html = r#"<table class="table table-condensed">
        <caption><strong>校园卡余额：0 元</strong></caption>
        <tbody>
            <tr><td>卡账号：</td><td>X</td></tr>
            <tr><td>挂失状态：</td><td>挂失</td></tr>
        </tbody>
    </table>"#;
    let ci = parse_balance(html).unwrap();
    assert_eq!(ci.lost_status, Some(CardLostStatus::Lost));
}

#[test]
fn unknown_status_warns_and_returns_none() {
    let html = r#"<table class="table table-condensed">
        <caption><strong>校园卡余额：0 元</strong></caption>
        <tbody>
            <tr><td>卡账号：</td><td>X</td></tr>
            <tr><td>挂失状态：</td><td>未知状态</td></tr>
        </tbody>
    </table>"#;
    let ci = parse_balance(html).unwrap();
    assert!(
        ci.lost_status.is_none(),
        "未知状态应 None: {:?}",
        ci.lost_status
    );
}

#[test]
fn extract_after_label_strips_nbsp() {
    // 真机 caption 文本通常是 "校园卡余额：\u{00A0}\u{00A0}\u{00A0}\u{00A0}3.88 元"
    let s = "校园卡余额：\u{00A0}\u{00A0}3.88 元";
    assert_eq!(
        extract_after_label(s, "校园卡余额：").as_deref(),
        Some("3.88 元")
    );
}
