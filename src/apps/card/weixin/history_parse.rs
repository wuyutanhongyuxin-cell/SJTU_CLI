//! `ecardbill.php` HTML → `Vec<Transaction>` + 可选 footer 汇总。
//!
//! HTML 结构（真机 dump 2026-05-19 D12-T1）：
//! - 主 table `<table class="table table-condensed">`（页内 banner 无 class）
//! - thead 三列：消费日期·地点 / 交易额 / 卡余额
//! - **tbody 缺 `<tr>` 包裹交易行**！直接是连续 3 个裸 `<td>`：
//!   - td_a: `<td><strong>YYYY-MM-DD HH:MM:SS</strong><br>系统<br>商户</td>`
//!     （转账行可能只到系统、无第三段）
//!   - td_b: 金额（充值正数包 `<font color="#FF0000">N</font>`）
//!   - td_c: 卡余额
//! - footer 用真 `<tr><td colspan=2>充值收入：</td><td>N&nbsp;元</td></tr>` + 消费支出
//!
//! Parser 策略：select 所有 `tbody td`，按文档顺序扫描。
//! 含 `<strong>YYYY-MM-DD HH:MM:SS</strong>` → 作 row anchor，下两 td 是金额 + 余额；
//! 有 `colspan` 属性 → 跳过（footer / 占位）；其它 → 跳过。
//! 不依赖 html5ever 自动补 `<tr>` 行为（不同 parser 版本不一致）。

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
    let table_sel = Selector::parse("table.table-condensed")
        .map_err(|e| anyhow!("CSS table.table-condensed：{e:?}"))?;
    let Some(table) = doc.select(&table_sel).next() else {
        return Ok(Vec::new());
    };
    let tbody_td_sel = Selector::parse("tbody td").map_err(|e| anyhow!("CSS tbody td：{e:?}"))?;
    let strong_sel = Selector::parse("strong").map_err(|e| anyhow!("CSS strong：{e:?}"))?;

    let tds: Vec<ElementRef> = table.select(&tbody_td_sel).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < tds.len() {
        let td = tds[i];
        // footer 行：colspan 属性 → 跳过
        if td.value().attr("colspan").is_some() {
            i += 1;
            continue;
        }
        // 行 anchor：含 <strong>YYYY-MM-DD HH:MM:SS</strong>
        let strong_text = td
            .select(&strong_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if NaiveDateTime::parse_from_str(&strong_text, "%Y-%m-%d %H:%M:%S").is_err() {
            i += 1;
            continue;
        }
        if i + 2 >= tds.len() {
            tracing::warn!("时间 td 后不足两个 td，丢弃");
            break;
        }
        match parse_one_record(&td, &tds[i + 1], &tds[i + 2]) {
            Ok(t) => out.push(t),
            Err(e) => tracing::warn!(error = %e, "跳过无法解析的流水行"),
        }
        i += 3;
    }
    Ok(out)
}

fn parse_one_record(
    time_td: &ElementRef<'_>,
    amount_td: &ElementRef<'_>,
    balance_td: &ElementRef<'_>,
) -> Result<Transaction> {
    let segments: Vec<String> = time_td
        .text()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let time_str = segments
        .first()
        .ok_or_else(|| anyhow!("time td 无文本片段"))?;
    let dt_naive =
        NaiveDateTime::parse_from_str(time_str, "%Y-%m-%d %H:%M:%S").context("交易时间解析")?;
    let offset = FixedOffset::east_opt(BEIJING_OFFSET_SECS)
        .ok_or_else(|| anyhow!("FixedOffset +08:00 构造失败"))?;
    let dt: DateTime<FixedOffset> = offset
        .from_local_datetime(&dt_naive)
        .single()
        .ok_or_else(|| anyhow!("本地时间到 FixedOffset 歧义"))?;
    let amount_text = amount_td.text().collect::<String>();
    let balance_text = balance_td.text().collect::<String>();
    let amount = parse_money_zh(&amount_text).context("amount 解析")?;
    let card_balance = parse_money_zh(&balance_text).context("card_balance 解析")?;
    Ok(Transaction {
        date_time_ms: dt.timestamp_millis(),
        date_tim_account_ms: None,
        system: segments.get(1).cloned(),
        merchant_no: None,
        merchant: segments.get(2).cloned(),
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

/// 解析 footer 汇总：扫所有 tr，匹配 `充值收入` / `消费支出` 前缀。
/// 真机 HTML 没有 `<tfoot>`，footer 是 tbody 内 `<tr><td colspan=2>...</td><td>N&nbsp;元</td></tr>`。
pub fn parse_history_summary(html: &str) -> HistorySummary {
    let none = HistorySummary {
        topup_total: None,
        spend_total: None,
    };
    let doc = Html::parse_document(html);
    let (Ok(table_sel), Ok(row_sel), Ok(td_sel)) = (
        Selector::parse("table.table-condensed"),
        Selector::parse("tr"),
        Selector::parse("td"),
    ) else {
        return none;
    };
    let Some(table) = doc.select(&table_sel).next() else {
        return none;
    };
    let mut topup = None;
    let mut spend = None;
    for tr in table.select(&row_sel) {
        let cells: Vec<String> = tr
            .select(&td_sel)
            .map(|e| e.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if cells.len() < 2 {
            continue;
        }
        let label = cells[0].as_str();
        let Some(value) = cells.last() else {
            continue;
        };
        if label.contains("充值收入") {
            topup = parse_money_zh(value).ok();
        } else if label.contains("消费支出") {
            spend = parse_money_zh(value).ok();
        }
    }
    HistorySummary {
        topup_total: topup,
        spend_total: spend,
    }
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
}
