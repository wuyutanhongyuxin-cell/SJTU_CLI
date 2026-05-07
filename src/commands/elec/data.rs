//! `sjtu elec <sub>` 的数据形状。每个 `cmd_*` 对应一个 `*Data` 结构。
//!
//! **金额硬约束**：所有 Decimal 字段都通过本地 `ser_decimal_str` 序列化为字符串，
//! 避开 JSON f64 精度坑（CLAUDE.md 项目专属约束）。
//!
//! **身份取舍**：`MeInfo` 里 `name` / `work_no` 是身份强标识，命令层默认**不放进
//! BalanceData**。只暴露 `room`（"D10-406" 这类位置标识）。

use rust_decimal::Decimal;
use serde::Serialize;

use crate::apps::elec::DailyUsage;

/// `sjtu elec balance` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct BalanceData {
    /// 房间名（"D10-406"），来自 `MeInfo.roomname`。
    pub room: Option<String>,
    /// 是否已绑定房间。`false` 时 balance 字段语义不明，CLI 仍输出但建议用户先 web 端绑定。
    pub isbind: bool,
    /// 剩余度数（kWh）。
    #[serde(with = "ser_decimal_str")]
    pub remaining_kwh: Decimal,
    /// 剩余补助度数。
    #[serde(with = "ser_decimal_str")]
    pub subsidy_kwh: Decimal,
    /// 剩余金额（元）。
    #[serde(with = "ser_decimal_str")]
    pub balance_yuan: Decimal,
    /// 剩余补助金额（元）。
    #[serde(with = "ser_decimal_str")]
    pub subsidy_balance_yuan: Decimal,
}

/// `sjtu elec usage [--month YYYY-MM]` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct UsageData {
    pub year: u16,
    pub month: u8,
    /// 上月用电度数。
    #[serde(with = "ser_decimal_str")]
    pub last_month_kwh: Decimal,
    /// 当月用电度数。
    #[serde(with = "ser_decimal_str")]
    pub this_month_kwh: Decimal,
}

/// `sjtu elec history [--days N]` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct HistoryData {
    /// 起始日期（含），`YYYY-MM-DD`。
    pub begin: String,
    /// 截止日期（含），`YYYY-MM-DD`。
    pub end: String,
    /// 实际返回条数（与 `items.len()` 一致）。
    pub returned: usize,
    /// `items` 累加。
    #[serde(with = "ser_decimal_str")]
    pub total_kwh: Decimal,
    /// 每日明细（`DailyUsage::used` 已通过 apps::elec::models 的 ser/de 输出字符串）。
    pub items: Vec<DailyUsage>,
}

/// `Decimal` → JSON 字符串。
mod ser_decimal_str {
    use rust_decimal::Decimal;
    use serde::Serializer;
    pub fn serialize<S: Serializer>(d: &Decimal, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&d.to_string())
    }
}
