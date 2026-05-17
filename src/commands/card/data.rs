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

/// 把 cardNo 脱敏成 "前 4 字符 + ***"。短于 5 个字符时整体 ***。
/// 使用 char-aware 切片，避免多字节 UTF-8 panic。
pub fn redact_card_no(s: &str) -> String {
    let n = s.chars().count();
    if n < 5 {
        "***".to_string()
    } else {
        let prefix: String = s.chars().take(4).collect();
        format!("{prefix}***")
    }
}

/// bankNo 脱敏：前 4 字符 + **** + 后 4 字符。短于 9 个字符时整体 ****。
/// 使用 char-aware 切片，避免多字节 UTF-8 panic。
pub fn redact_bank_no(s: &str) -> String {
    let n = s.chars().count();
    if n < 9 {
        "****".to_string()
    } else {
        let prefix: String = s.chars().take(4).collect();
        let suffix: String = s.chars().skip(n - 4).collect();
        format!("{prefix}****{suffix}")
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

    #[test]
    fn redact_handles_utf8_multibyte() {
        // 字节长度 >= 5 但前 4 字节落在多字节 char 中间会 panic（旧实现）
        // 用中文字符验证 char-aware 实现正确性
        assert_eq!(redact_card_no("你好世界abc"), "你好世界***");
        assert_eq!(redact_bank_no("你好世界中abcde"), "你好世界****bcde");
    }
}
