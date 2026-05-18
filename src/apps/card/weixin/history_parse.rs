//! `ecardbill.php` HTML → `Vec<Transaction>` + 可选 footer 汇总。
//!
//! 真机 HTML 结构：`<table class="history"><thead>...</thead><tbody><tr>列5</tr></tbody><tfoot>...</tfoot></table>`。
//! 列序：交易时间 / 系统 / 商户 / 金额 / 卡余额（按真机抓取 2026-05-17）。
//!
//! datetime 解析为 `+08:00 FixedOffset`，serialize 为 ISO8601 字符串（与 OAuth2 path 归一）。

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};
use rust_decimal::Decimal;
use scraper::{ElementRef, Html, Selector};

use super::money::parse_money_zh;
use crate::apps::card::models::Transaction;

const BEIJING_OFFSET_SECS: i32 = 8 * 3600;

/// 解析 ecardbill.php HTML，返回交易记录列表（按 HTML 行序保留，最新在前）。
pub fn parse_history(html: &str) -> Result<Vec<Transaction>> {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("tbody tr").map_err(|e| anyhow!("CSS tbody tr：{e:?}"))?;
    let td_sel = Selector::parse("td").map_err(|e| anyhow!("CSS td：{e:?}"))?;

    let mut out = Vec::new();
    for tr in doc.select(&row_sel) {
        match parse_one_row(&tr, &td_sel) {
            Ok(t) => out.push(t),
            Err(e) => {
                tracing::warn!(error = %e, "跳过无法解析的流水行");
            }
        }
    }
    Ok(out)
}

fn parse_one_row(tr: &ElementRef<'_>, td_sel: &Selector) -> Result<Transaction> {
    let tds: Vec<String> = tr
        .select(td_sel)
        .map(|e| e.text().collect::<String>().trim().to_string())
        .collect();
    if tds.len() < 5 {
        return Err(anyhow!("流水行列数 {} < 5", tds.len()));
    }
    let dt_naive =
        NaiveDateTime::parse_from_str(&tds[0], "%Y-%m-%d %H:%M:%S").context("交易时间解析")?;
    let offset = FixedOffset::east_opt(BEIJING_OFFSET_SECS)
        .ok_or_else(|| anyhow!("FixedOffset +08:00 构造失败"))?;
    let dt: DateTime<FixedOffset> = offset
        .from_local_datetime(&dt_naive)
        .single()
        .ok_or_else(|| anyhow!("本地时间到 FixedOffset 歧义"))?;

    let amount = parse_money_zh(&tds[3]).context("amount 解析")?;
    let card_balance = parse_money_zh(&tds[4]).context("card_balance 解析")?;

    Ok(Transaction {
        date_time_ms: dt.timestamp_millis(),
        date_tim_account_ms: None,
        system: if tds[1].is_empty() {
            None
        } else {
            Some(tds[1].clone())
        },
        merchant_no: None,
        merchant: if tds[2].is_empty() {
            None
        } else {
            Some(tds[2].clone())
        },
        description: None,
        amount,
        card_balance,
    })
}

/// footer 汇总：充值总额 + 消费总额。无 footer 或解析失败 → 返 None 字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistorySummary {
    pub topup_total: Option<Decimal>,
    pub spend_total: Option<Decimal>,
}

/// 解析页底 `<tfoot>` 汇总。无 footer 或解析失败 → 返 None 字段。
pub fn parse_history_summary(html: &str) -> HistorySummary {
    let doc = Html::parse_document(html);
    let tfoot_sel = match Selector::parse("tfoot td") {
        Ok(s) => s,
        Err(_) => {
            return HistorySummary {
                topup_total: None,
                spend_total: None,
            }
        }
    };
    let text: String = doc
        .select(&tfoot_sel)
        .map(|e| e.text().collect::<String>())
        .collect();
    HistorySummary {
        topup_total: extract_after(&text, "充值").and_then(|s| parse_money_zh(&s).ok()),
        spend_total: extract_after(&text, "消费").and_then(|s| parse_money_zh(&s).ok()),
    }
}

/// 从 `"充值 20 元 / 消费 -16.3 元"` 中按 `prefix` 截取金额片段。
fn extract_after(text: &str, prefix: &str) -> Option<String> {
    let idx = text.find(prefix)?;
    let rest = &text[idx + prefix.len()..];
    let end = rest.find('/').unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(t0.system.as_deref(), Some("六期水控"));
        assert_eq!(t0.merchant.as_deref(), Some("六期水控"));
    }

    #[test]
    fn topup_row_positive_amount() {
        let v = parse_history(&fixture()).unwrap();
        let t2 = &v[2];
        assert_eq!(t2.amount, Decimal::from(20));
        assert_eq!(t2.system.as_deref(), Some("银行转账"));
    }

    #[test]
    fn datetime_serialized_as_beijing_ms() {
        let v = parse_history(&fixture()).unwrap();
        // 2026-05-17 00:41:00 +08:00 → UTC 2026-05-16 16:41:00 → ms
        let expected = chrono::FixedOffset::east_opt(8 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 5, 17, 0, 41, 0)
            .unwrap()
            .timestamp_millis();
        assert_eq!(v[0].date_time_ms, expected);
    }

    #[test]
    fn empty_tbody_returns_empty_vec() {
        let html = r#"<table><tbody></tbody></table>"#;
        let v = parse_history(html).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn malformed_row_skipped() {
        let html = r#"<table><tbody>
            <tr><td>bad-date</td><td>x</td><td>y</td><td>1</td><td>2</td></tr>
            <tr><td>2026-05-17 00:00:00</td><td>x</td><td>y</td><td>1</td><td>2</td></tr>
        </tbody></table>"#;
        let v = parse_history(html).unwrap();
        assert_eq!(v.len(), 1, "坏行跳过，好行保留");
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
        let html = r#"<table><tbody></tbody></table>"#;
        let s = parse_history_summary(html);
        assert!(s.topup_total.is_none());
        assert!(s.spend_total.is_none());
    }
}
