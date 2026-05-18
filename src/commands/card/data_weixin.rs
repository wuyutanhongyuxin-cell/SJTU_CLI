//! weixin path → `BalanceData` / `HistoryData` 转换器。
//! weixin path 不经 OAuth2，PII 字段较 OAuth2 path 少。映射 `apps::card::models` → 命令层输出。

use chrono::{FixedOffset, TimeZone};
use rust_decimal::Decimal;

use super::data::{redact_card_no, BalanceData, HistoryData, TransactionItem};

impl BalanceData {
    /// 从 weixin path `CardInfo` 构造 BalanceData。
    ///
    /// **PII 红线**：`user` / `bank_no_redacted` / `face_sub_type` 永 `None`。
    /// `expire_date` / `face_type` weixin HTML 不出，也为 `None`。
    /// `lost` / `frozen` 由 `lost_status` / `freeze_status` enum 翻译为 bool。
    pub fn from_weixin_card_info(
        ci: &crate::apps::card::models::CardInfo,
        elapsed_ms: u128,
    ) -> Self {
        use crate::apps::card::models::{CardFreezeStatus, CardLostStatus};
        let lost = matches!(ci.lost_status, Some(CardLostStatus::Lost));
        let frozen = matches!(ci.freeze_status, Some(CardFreezeStatus::Frozen));
        Self {
            card_no_redacted: redact_card_no(&ci.card_no),
            balance: ci.card_balance,
            trans_balance: ci.trans_balance,
            expire_date: None,
            lost,
            frozen,
            face_type: None,
            face_sub_type: None,
            user: None,
            bank_no_redacted: None,
            from_cache: false,
            elapsed_ms,
        }
    }
}

impl HistoryData {
    /// 从 weixin path `Vec<Transaction>` 构造 HistoryData。
    ///
    /// `card_no_redacted` 用 `<weixin>` 占位 —— weixin path fetch_history 不返卡号，
    /// 占位明示"路径不识别个体卡"。Agent 据 `envelope.meta.via=weixin` 判断该字段语义。
    pub fn from_weixin_transactions(
        txs: &[crate::apps::card::models::Transaction],
        begin_local: chrono::NaiveDate,
        end_local: chrono::NaiveDate,
        elapsed_ms: u128,
    ) -> Self {
        let beijing = FixedOffset::east_opt(8 * 3600).expect("+08:00");
        let items: Vec<TransactionItem> = txs
            .iter()
            .map(|t| TransactionItem {
                consumed_at: beijing
                    .timestamp_millis_opt(t.date_time_ms)
                    .single()
                    .unwrap_or_else(|| {
                        beijing
                            .timestamp_millis_opt(0)
                            .single()
                            .expect("epoch always valid")
                    }),
                system: t.system.clone(),
                merchant_no: t.merchant_no.clone(),
                merchant: t.merchant.clone(),
                description: t.description.clone(),
                amount: t.amount,
                balance_after: t.card_balance,
            })
            .collect();
        let total_amount: Decimal = items.iter().map(|t| t.amount).sum();
        let total = items.len() as u64;
        let returned = items.len();
        Self {
            card_no_redacted: "<weixin>".to_string(),
            begin_date_local: begin_local.format("%Y-%m-%d").to_string(),
            end_date_local: end_local.format("%Y-%m-%d").to_string(),
            returned,
            total,
            transactions: items,
            total_amount,
            from_cache: false,
            elapsed_ms,
        }
    }
}

#[cfg(test)]
#[path = "data_weixin_tests.rs"]
mod tests;
