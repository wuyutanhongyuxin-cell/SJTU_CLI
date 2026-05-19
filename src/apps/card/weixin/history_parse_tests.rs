//! `history_parse` 兄弟测试文件（拆自 history_parse.rs，避免单文件超 200 行硬限）。

#![cfg(test)]

use super::history_parse::{parse_history, parse_history_summary};
use chrono::TimeZone;
use rust_decimal::Decimal;

fn fixture() -> String {
    std::fs::read_to_string("tests/fixtures/card_history_weixin.html").expect("读 fixture 失败")
}

#[test]
fn parses_three_rows() {
    let v = parse_history(&fixture()).unwrap();
    assert_eq!(v.len(), 3, "应解析 3 条记录");
}

#[test]
fn first_row_field_values() {
    let v = parse_history(&fixture()).unwrap();
    let t0 = &v[0];
    assert_eq!(t0.amount, Decimal::from_str_exact("-0.8").unwrap());
    assert_eq!(t0.card_balance, Decimal::from_str_exact("3.88").unwrap());
    assert_eq!(t0.system.as_deref(), Some("示例商户A"));
    assert_eq!(t0.merchant.as_deref(), Some("示例商户A"));
}

#[test]
fn topup_row_positive_amount_no_merchant() {
    let v = parse_history(&fixture()).unwrap();
    let t2 = &v[2];
    assert_eq!(t2.amount, Decimal::from(20));
    assert_eq!(t2.system.as_deref(), Some("银行转账"));
    assert!(t2.merchant.is_none(), "转账行无商户：{:?}", t2.merchant);
}

#[test]
fn datetime_serialized_as_beijing_ms() {
    let v = parse_history(&fixture()).unwrap();
    // 2026-05-17 00:41:00 +08:00
    let expected = chrono::FixedOffset::east_opt(8 * 3600)
        .unwrap()
        .with_ymd_and_hms(2026, 5, 17, 0, 41, 0)
        .unwrap()
        .timestamp_millis();
    assert_eq!(v[0].date_time_ms, expected);
}

#[test]
fn empty_tbody_returns_empty_vec() {
    let html = r#"<table class="table table-condensed"><tbody></tbody></table>"#;
    let v = parse_history(html).unwrap();
    assert!(v.is_empty());
}

#[test]
fn missing_table_class_returns_empty() {
    // 缺主 table 不应 panic，返空 vec 即可（caller 上层会 detect_stale 兜底）
    let html = r#"<html><body><p>nothing</p></body></html>"#;
    let v = parse_history(html).unwrap();
    assert!(v.is_empty());
}

#[test]
fn malformed_row_skipped() {
    // 时间无效 → 跳过；好行保留
    let html = r#"<table class="table table-condensed"><tbody>
        <td><strong>bad-date</strong></td><td>x</td><td>y</td>
        <td><strong>2026-05-17 00:00:00</strong></td><td>-1</td><td>10</td>
    </tbody></table>"#;
    let v = parse_history(html).unwrap();
    assert_eq!(v.len(), 1, "坏行跳过，好行保留");
}

#[test]
fn footer_row_with_colspan_ignored() {
    // colspan=3 占位行不该被识别为交易
    let html = r#"<table class="table table-condensed"><tbody>
        <tr><td colspan="3">&nbsp;</td></tr>
        <td><strong>2026-05-17 00:00:00</strong></td><td>-1</td><td>10</td>
    </tbody></table>"#;
    let v = parse_history(html).unwrap();
    assert_eq!(v.len(), 1);
}

#[test]
fn footer_summary_parsed() {
    let s = parse_history_summary(&fixture());
    assert_eq!(s.topup_total, Some(Decimal::from(20)));
    assert_eq!(
        s.spend_total,
        Some(Decimal::from_str_exact("-16.3").unwrap())
    );
}

#[test]
fn footer_summary_missing_returns_none_fields() {
    let html = r#"<table class="table table-condensed"><tbody></tbody></table>"#;
    let s = parse_history_summary(html);
    assert!(s.topup_total.is_none());
    assert!(s.spend_total.is_none());
}
