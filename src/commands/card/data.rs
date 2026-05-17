//! `sjtu card balance` / `history` 输出数据结构。
//!
//! 默认输出抹身份字段（PII）；`--with-identity` 才输出 user / bank_no / faceSubType。
//! 物理卡号 `cardId` 永久不出（即便 `--with-identity`，防卡号克隆攻击面 — spec §8 红线）。
//! 金额一律 Decimal，序列化为字符串（避 f64 精度坑）。

use chrono::{DateTime, FixedOffset};
use rust_decimal::Decimal;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct BalanceData {
    pub card_no_redacted: String,
    pub balance: Decimal,
    pub trans_balance: Decimal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_date: Option<String>,
    pub lost: bool,
    pub frozen: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face_type: Option<String>,
    /// 含身份描述（"硕士研究生"）→ 仅 `--with-identity`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face_sub_type: Option<String>,
    /// `--with-identity` 才填
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<UserIdentity>,
    /// `--with-identity` 才填，前 4 + `****` + 后 4
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bank_no_redacted: Option<String>,
    pub from_cache: bool,
    pub elapsed_ms: u128,
}

#[derive(Debug, Serialize)]
pub struct UserIdentity {
    pub code: String,
    pub name: String,
    pub organize: String,
}

#[derive(Debug, Serialize)]
pub struct HistoryData {
    pub card_no_redacted: String,
    pub begin_date_local: String,
    pub end_date_local: String,
    pub returned: usize,
    pub total: u64,
    pub transactions: Vec<TransactionItem>,
    pub total_amount: Decimal,
    pub from_cache: bool,
    pub elapsed_ms: u128,
}

#[derive(Debug, Serialize)]
pub struct TransactionItem {
    /// `+08:00` 时区的消费时间，ISO 8601
    pub consumed_at: DateTime<FixedOffset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merchant_no: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merchant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub amount: Decimal,
    pub balance_after: Decimal,
}

/// 把 cardNo 脱敏成 "前 4 + ***"。短于 5 字符时整体 ***。
pub fn redact_card_no(s: &str) -> String {
    if s.len() < 5 {
        "***".to_string()
    } else {
        format!("{}***", &s[..4])
    }
}

/// bankNo 脱敏：前 4 + **** + 后 4。短于 9 字符时整体 ****。
pub fn redact_bank_no(s: &str) -> String {
    if s.len() < 9 {
        "****".to_string()
    } else {
        format!("{}****{}", &s[..4], &s[s.len() - 4..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_card_no_normal() {
        assert_eq!(redact_card_no("0012345678"), "0012***");
    }

    #[test]
    fn redact_card_no_short() {
        assert_eq!(redact_card_no("123"), "***");
        assert_eq!(redact_card_no(""), "***");
    }

    #[test]
    fn redact_bank_no_normal() {
        assert_eq!(redact_bank_no("6228000011112222"), "6228****2222");
    }

    #[test]
    fn redact_bank_no_short() {
        assert_eq!(redact_bank_no("12345678"), "****");
    }
}
