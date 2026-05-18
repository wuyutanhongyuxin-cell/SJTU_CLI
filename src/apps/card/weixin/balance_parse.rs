//! `ecardbalance.php` HTML → `CardInfo`。
//!
//! HTML 结构（真机调研 2026-05-17）：`<table class="info-table">` 行 = `<tr><th>字段名</th><td>值</td></tr>`。
//! 用 scraper 按 `<th>` 文本 anchor 抽 `<td>` 内容（不依赖 class/id，未来 HTML 改版风险 OQ-WX-3）。
//!
//! PII（姓名 / 学号）**不写入** CardInfo —— 解析时主动 drop。绑定银行卡走 OAuth2 既有 redact 路径。

use anyhow::{anyhow, Context, Result};
use rust_decimal::Decimal;
use scraper::{Html, Selector};

use super::money::parse_money_zh;
use crate::apps::card::models::{CardFreezeStatus, CardInfo, CardLostStatus};

/// 解析 ecardbalance.php HTML 主体为 CardInfo。
///
/// 必有字段：`卡账号` / `校园卡余额`。缺失抛 UpstreamError。
/// 可选字段：`过渡余额` / `挂失状态` / `冻结状态` 缺失 → warn + 用合理默认（ZERO/Normal）。
pub fn parse_balance(html: &str) -> Result<CardInfo> {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("tr").map_err(|e| anyhow!("CSS tr 选择器：{e:?}"))?;
    let th_sel = Selector::parse("th").map_err(|e| anyhow!("CSS th 选择器：{e:?}"))?;
    let td_sel = Selector::parse("td").map_err(|e| anyhow!("CSS td 选择器：{e:?}"))?;

    let mut card_no: Option<String> = None;
    let mut card_balance: Option<Decimal> = None;
    let mut trans_balance: Decimal = Decimal::ZERO;
    let mut lost: Option<CardLostStatus> = None;
    let mut frozen: Option<CardFreezeStatus> = None;

    for tr in doc.select(&row_sel) {
        let label_str = tr
            .select(&th_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string());
        let value_str = tr
            .select(&td_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string());
        match (label_str.as_deref(), value_str.as_deref()) {
            (Some("卡账号"), Some(v)) => card_no = Some(v.to_string()),
            (Some("校园卡余额"), Some(v)) => {
                card_balance = Some(parse_money_zh(v).context("校园卡余额解析")?)
            }
            (Some("过渡余额"), Some(v)) => {
                trans_balance = parse_money_zh(v).unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "过渡余额解析失败，回退 0");
                    Decimal::ZERO
                });
            }
            (Some("挂失状态"), Some(v)) => lost = parse_lost_status(v),
            (Some("冻结状态"), Some(v)) => frozen = parse_freeze_status(v),
            _ => {} // 姓名 / 学号 / 绑定银行卡 / 其它行：丢弃
        }
    }

    let card_no = card_no.ok_or_else(|| anyhow!("HTML 缺失『卡账号』字段"))?;
    let card_balance = card_balance.ok_or_else(|| anyhow!("HTML 缺失『校园卡余额』字段"))?;

    Ok(CardInfo {
        user: None,
        card_no,
        card_id: None,
        bank_no: None,
        expire_date: None,
        card_balance,
        trans_balance,
        lost: false,
        frozen: false,
        face_type: None,
        face_sub_type: None,
        lost_status: lost,
        freeze_status: frozen,
    })
}

fn parse_lost_status(s: &str) -> Option<CardLostStatus> {
    match s.trim() {
        "正常" => Some(CardLostStatus::Normal),
        "挂失" => Some(CardLostStatus::Lost),
        _ => {
            tracing::warn!(value = s, "未知挂失状态字符串");
            None
        }
    }
}

fn parse_freeze_status(s: &str) -> Option<CardFreezeStatus> {
    match s.trim() {
        "正常" => Some(CardFreezeStatus::Normal),
        "冻结" => Some(CardFreezeStatus::Frozen),
        _ => {
            tracing::warn!(value = s, "未知冻结状态字符串");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn missing_card_balance_errors() {
        let html = r#"<table><tr><th>卡账号</th><td>X</td></tr></table>"#;
        let r = parse_balance(html);
        assert!(r.is_err());
        let msg = format!("{:#}", r.unwrap_err());
        assert!(msg.contains("校园卡余额"), "错误应提及字段：{msg}");
    }

    #[test]
    fn missing_card_no_errors() {
        let html = r#"<table><tr><th>校园卡余额</th><td>1 元</td></tr></table>"#;
        let r = parse_balance(html);
        assert!(r.is_err());
        let msg = format!("{:#}", r.unwrap_err());
        assert!(msg.contains("卡账号"), "错误应提及字段：{msg}");
    }

    #[test]
    fn lost_status_lost_variant() {
        let html = r#"<table>
            <tr><th>卡账号</th><td>X</td></tr>
            <tr><th>校园卡余额</th><td>0 元</td></tr>
            <tr><th>挂失状态</th><td>挂失</td></tr>
        </table>"#;
        let ci = parse_balance(html).unwrap();
        assert_eq!(ci.lost_status, Some(CardLostStatus::Lost));
    }

    #[test]
    fn unknown_status_warns_and_returns_none() {
        let html = r#"<table>
            <tr><th>卡账号</th><td>X</td></tr>
            <tr><th>校园卡余额</th><td>0 元</td></tr>
            <tr><th>挂失状态</th><td>未知状态</td></tr>
        </table>"#;
        let ci = parse_balance(html).unwrap();
        assert!(
            ci.lost_status.is_none(),
            "未知状态应 None: {:?}",
            ci.lost_status
        );
    }
}
