//! T9 weixin 转换器单测（从 data_weixin.rs 拆出，避超 200 行硬限）。

use crate::apps::card::models::{CardFreezeStatus, CardInfo, CardLostStatus, Transaction};
use crate::commands::card::data::{BalanceData, HistoryData};
use rust_decimal::Decimal;

fn fake_weixin_card_info(
    lost: Option<CardLostStatus>,
    frozen: Option<CardFreezeStatus>,
) -> CardInfo {
    CardInfo {
        user: None,
        card_no: "123456".into(),
        card_id: None,
        bank_no: None,
        expire_date: None,
        card_balance: Decimal::from_str_exact("3.88").unwrap(),
        trans_balance: Decimal::ZERO,
        lost: false,
        frozen: false,
        face_type: None,
        face_sub_type: None,
        lost_status: lost,
        freeze_status: frozen,
    }
}

#[test]
fn balance_data_from_weixin_normal_status() {
    let ci = fake_weixin_card_info(Some(CardLostStatus::Normal), Some(CardFreezeStatus::Normal));
    let bd = BalanceData::from_weixin_card_info(&ci, 42);
    assert!(!bd.lost);
    assert!(!bd.frozen);
    assert!(bd.user.is_none(), "PII 红线：user 永 None");
    assert!(bd.bank_no_redacted.is_none(), "PII 红线：bank_no 永 None");
    assert!(
        bd.face_sub_type.is_none(),
        "PII 红线：face_sub_type 永 None"
    );
    assert_eq!(bd.elapsed_ms, 42);
    assert!(!bd.from_cache);
    assert_eq!(bd.card_no_redacted, "1234***");
    assert_eq!(bd.balance, Decimal::from_str_exact("3.88").unwrap());
}

#[test]
fn balance_data_from_weixin_lost_card_maps_to_bool() {
    let ci = fake_weixin_card_info(Some(CardLostStatus::Lost), Some(CardFreezeStatus::Normal));
    let bd = BalanceData::from_weixin_card_info(&ci, 0);
    assert!(bd.lost, "lost_status=Lost 应映射 lost=true");
    assert!(!bd.frozen);
}

#[test]
fn balance_data_from_weixin_frozen_card_maps_to_bool() {
    let ci = fake_weixin_card_info(Some(CardLostStatus::Normal), Some(CardFreezeStatus::Frozen));
    let bd = BalanceData::from_weixin_card_info(&ci, 0);
    assert!(!bd.lost);
    assert!(bd.frozen, "freeze_status=Frozen 应映射 frozen=true");
}

#[test]
fn balance_data_from_weixin_no_status_defaults_false() {
    let ci = fake_weixin_card_info(None, None);
    let bd = BalanceData::from_weixin_card_info(&ci, 0);
    assert!(!bd.lost, "无 lost_status 默认 false");
    assert!(!bd.frozen, "无 freeze_status 默认 false");
}

#[test]
fn history_data_from_weixin_uses_placeholder_card_no() {
    let begin = chrono::NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2026, 5, 31).unwrap();
    let hd = HistoryData::from_weixin_transactions(&[], begin, end, 99);
    assert_eq!(hd.card_no_redacted, "<weixin>", "weixin path 占位字符串");
    assert_eq!(hd.begin_date_local, "2026-05-01");
    assert_eq!(hd.end_date_local, "2026-05-31");
    assert_eq!(hd.returned, 0);
    assert_eq!(hd.total, 0);
    assert_eq!(hd.total_amount, Decimal::ZERO);
    assert_eq!(hd.elapsed_ms, 99);
    assert!(!hd.from_cache);
}

#[test]
fn history_data_from_weixin_converts_transactions() {
    use chrono::TimeZone;
    let beijing = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let ms = beijing
        .with_ymd_and_hms(2026, 5, 17, 0, 41, 0)
        .unwrap()
        .timestamp_millis();
    let txs = vec![
        Transaction {
            date_time_ms: ms,
            date_tim_account_ms: None,
            system: Some("六期水控".into()),
            merchant_no: None,
            merchant: Some("六期水控".into()),
            description: None,
            amount: Decimal::from_str_exact("-0.8").unwrap(),
            card_balance: Decimal::from_str_exact("3.88").unwrap(),
        },
        Transaction {
            date_time_ms: ms,
            date_tim_account_ms: None,
            system: None,
            merchant_no: None,
            merchant: None,
            description: None,
            amount: Decimal::from(20),
            card_balance: Decimal::from_str_exact("23.88").unwrap(),
        },
    ];
    let begin = chrono::NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2026, 5, 31).unwrap();
    let hd = HistoryData::from_weixin_transactions(&txs, begin, end, 0);
    assert_eq!(hd.transactions.len(), 2);
    assert_eq!(hd.returned, 2);
    assert_eq!(hd.total, 2);
    assert_eq!(
        hd.transactions[0].amount,
        Decimal::from_str_exact("-0.8").unwrap()
    );
    assert_eq!(
        hd.transactions[0].balance_after,
        Decimal::from_str_exact("3.88").unwrap()
    );
    assert_eq!(hd.transactions[0].system.as_deref(), Some("六期水控"));
    // total_amount = -0.8 + 20 = 19.2
    assert_eq!(hd.total_amount, Decimal::from_str_exact("19.2").unwrap());
}
