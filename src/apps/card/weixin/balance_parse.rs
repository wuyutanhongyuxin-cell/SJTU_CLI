//! `ecardbalance.php` HTML → `CardInfo`。
//!
//! HTML 结构（真机 dump 2026-05-19 D12-T1）：
//! - 主 table 是页内第 2 个，`<table class="table table-condensed">`（第一个是顶蓝条 banner，无 class）
//! - 余额放在 `<caption><strong>校园卡余额：&nbsp;&nbsp;&nbsp;&nbsp;3.88 元</strong></caption>`
//! - 字段 rows 在 `<tbody><tr><td>字段名：</td><td>值</td></tr>`（两个 td、label 含全角 `：`）
//!
//! 用 scraper 按 label 文本 anchor 抽 td 内容（不依赖 class/id，未来 HTML 改版风险 OQ-WX-3）。
//!
//! PII（姓名 / 学号 / 绑定银行卡）**不写入** CardInfo —— 解析时主动 drop。

use anyhow::{anyhow, Context, Result};
use rust_decimal::Decimal;
use scraper::{Html, Selector};

use super::money::parse_money_zh;
use crate::apps::card::models::{CardFreezeStatus, CardInfo, CardLostStatus};

/// 解析 ecardbalance.php HTML 主体为 CardInfo。
///
/// 必有字段：caption 的『校园卡余额：N 元』 + `卡账号：` row。缺失抛 anyhow Err。
/// 可选字段：`过渡余额：` / `挂失状态：` / `冻结状态：` 缺失 → warn + 用合理默认。
pub fn parse_balance(html: &str) -> Result<CardInfo> {
    let doc = Html::parse_document(html);
    let table_sel = Selector::parse("table.table-condensed")
        .map_err(|e| anyhow!("CSS table.table-condensed：{e:?}"))?;
    let table = doc
        .select(&table_sel)
        .next()
        .ok_or_else(|| anyhow!("HTML 缺失 table.table-condensed"))?;

    let caption_strong_sel =
        Selector::parse("caption strong").map_err(|e| anyhow!("CSS caption strong：{e:?}"))?;
    let caption_text = table
        .select(&caption_strong_sel)
        .next()
        .map(|e| e.text().collect::<String>())
        .ok_or_else(|| anyhow!("HTML 缺失『校园卡余额』caption"))?;
    let card_balance = extract_after_label(&caption_text, "校园卡余额：")
        .ok_or_else(|| anyhow!("caption 中未找到『校园卡余额：N 元』形态"))
        .and_then(|s| parse_money_zh(&s).context("校园卡余额解析"))?;

    let row_sel = Selector::parse("tbody tr").map_err(|e| anyhow!("CSS tbody tr：{e:?}"))?;
    let td_sel = Selector::parse("td").map_err(|e| anyhow!("CSS td：{e:?}"))?;

    let mut card_no: Option<String> = None;
    let mut trans_balance: Decimal = Decimal::ZERO;
    let mut lost: Option<CardLostStatus> = None;
    let mut frozen: Option<CardFreezeStatus> = None;

    for tr in table.select(&row_sel) {
        let tds: Vec<String> = tr
            .select(&td_sel)
            .map(|e| e.text().collect::<String>().trim().to_string())
            .collect();
        if tds.len() < 2 {
            continue;
        }
        match tds[0].as_str() {
            "卡账号：" => card_no = Some(tds[1].clone()),
            "过渡余额：" => {
                trans_balance = parse_money_zh(&tds[1]).unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "过渡余额解析失败，回退 0");
                    Decimal::ZERO
                });
            }
            "挂失状态：" => lost = parse_lost_status(&tds[1]),
            "冻结状态：" => frozen = parse_freeze_status(&tds[1]),
            _ => {} // 姓名 / 绑定银行卡 / 其它：丢弃（PII 红线）
        }
    }

    let card_no = card_no.ok_or_else(|| anyhow!("HTML 缺失『卡账号』字段"))?;

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

/// 从 `"校园卡余额：&nbsp;&nbsp;3.88 元"` 中按 prefix 截取金额片段。
/// 服务端用 `&nbsp;` (U+00A0) 缩进，先全文替换 nbsp 再 trim 避免歧义。
///
/// `pub(super)` 让 sibling tests 文件能调（仅测试用途，业务不跨文件调用）。
pub(super) fn extract_after_label(text: &str, prefix: &str) -> Option<String> {
    let idx = text.find(prefix)?;
    let rest = &text[idx + prefix.len()..];
    Some(rest.replace('\u{00A0}', " ").trim().to_string())
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

// tests 拆到 sibling 文件 `balance_parse_tests.rs`（避免单文件超 200 行硬限）。
// 在父模块 `weixin/mod.rs` 内 `#[cfg(test)] mod balance_parse_tests;` 挂载。
