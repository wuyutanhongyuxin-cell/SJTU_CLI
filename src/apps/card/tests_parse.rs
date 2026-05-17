//! `apps::card::models` 解析单元测试（拆出以满足 ≤200 行限制）。

use super::models::{CardInfo, Envelope, Transaction};
use rust_decimal::Decimal;

#[test]
fn parse_card_info_full_fields() {
    let raw = r#"{
        "entities": [{
            "user": {"code":"123","name":"张三","organize":{"name":"电信"}},
            "cardNo": "0012345",
            "cardId": "AABBCC",
            "bankNo": "6228000011112222",
            "expireDate": "20601231",
            "cardBalance": 284.25,
            "transBalance": 0.00,
            "lost": false,
            "frozen": false,
            "faceType": "1",
            "faceSubType": "硕士研究生"
        }]
    }"#;
    let env: Envelope<CardInfo> = serde_json::from_str(raw).unwrap();
    assert_eq!(env.entities.len(), 1);
    let c = &env.entities[0];
    assert_eq!(c.card_no, "0012345");
    assert_eq!(c.card_balance, Decimal::from_str_exact("284.25").unwrap());
    assert_eq!(c.trans_balance, Decimal::from_str_exact("0.00").unwrap());
    assert_eq!(c.user.as_ref().unwrap().name.as_deref(), Some("张三"));
    assert_eq!(c.face_sub_type.as_deref(), Some("硕士研究生"));
}

#[test]
fn parse_card_info_minimal() {
    let raw = r#"{"entities":[{"cardNo":"X","cardBalance":0,"transBalance":0}]}"#;
    let env: Envelope<CardInfo> = serde_json::from_str(raw).unwrap();
    let c = &env.entities[0];
    assert_eq!(c.card_no, "X");
    assert!(c.user.is_none());
    assert!(!c.lost && !c.frozen);
}

#[test]
fn parse_transactions_with_spelling_trap() {
    // 注意服务端字段是 dateTimAccount（少 e）
    let raw = r#"{
        "total": 2,
        "entities": [
            {"dateTime": 1715750000000, "dateTimAccount": 1715760000000,
             "system": "S", "merchantNo":"M1", "merchant":"大众餐厅",
             "description":"持卡人消费", "amount": -10.66, "cardBalance": 273.59},
            {"dateTime": 1715840000000,
             "system": "S", "merchant":"宿舍洗衣机",
             "description":"持卡人消费", "amount": -2.0, "cardBalance": 271.59}
        ]
    }"#;
    let env: Envelope<Transaction> = serde_json::from_str(raw).unwrap();
    assert_eq!(env.total, Some(2));
    assert_eq!(env.entities.len(), 2);
    let t0 = &env.entities[0];
    assert_eq!(t0.amount, Decimal::from_str_exact("-10.66").unwrap());
    assert_eq!(t0.date_tim_account_ms, Some(1715760000000));
    let t1 = &env.entities[1];
    assert_eq!(t1.amount, Decimal::from_str_exact("-2.0").unwrap());
    assert_eq!(t1.date_tim_account_ms, None);
}

#[test]
fn parse_transactions_empty() {
    let raw = r#"{"total": 0, "entities": []}"#;
    let env: Envelope<Transaction> = serde_json::from_str(raw).unwrap();
    assert_eq!(env.total, Some(0));
    assert_eq!(env.entities.len(), 0);
}

#[test]
fn parse_envelope_with_errno_10002() {
    let raw = r#"{"errno": 10002, "error": "Authentication Failed", "total": 0}"#;
    let env: Envelope<CardInfo> = serde_json::from_str(raw).unwrap();
    assert_eq!(env.errno, Some(10002));
    assert_eq!(env.error.as_deref(), Some("Authentication Failed"));
    assert_eq!(env.entities.len(), 0);
}

#[test]
fn negative_amount_serialized_as_string() {
    let t = Transaction {
        date_time_ms: 0,
        date_tim_account_ms: None,
        system: None,
        merchant_no: None,
        merchant: None,
        description: None,
        amount: Decimal::from_str_exact("-10.66").unwrap(),
        card_balance: Decimal::from_str_exact("273.59").unwrap(),
    };
    let s = serde_json::to_string(&t).unwrap();
    assert!(s.contains(r#""amount":"-10.66""#), "actual: {s}");
    assert!(s.contains(r#""cardBalance":"273.59""#), "actual: {s}");
}
