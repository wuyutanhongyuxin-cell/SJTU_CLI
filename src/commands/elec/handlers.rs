//! `sjtu elec <sub>` handler：MVP 三件套 balance / usage / history。
//!
//! 都**只读**：CAS connect → GET → 渲染 Envelope；不点任何"立即支付"按钮（§7.6 硬红线）。

use anyhow::Result;
use chrono::{Datelike, Duration};
use rust_decimal::Decimal;

use super::data::{BalanceData, HistoryData, UsageData};
use crate::apps::elec::Client;
use crate::error::SjtuCliError;
use crate::output::{render, Envelope, OutputFormat};

/// `sjtu elec balance`：调 `/api/me/info` + `/api/ws/sydl`，输出绑定房间 + 4 个余额字段。
pub async fn cmd_balance(fmt: Option<OutputFormat>) -> Result<()> {
    let client = Client::connect().await?;
    let me = client.me_info().await?;
    let bal = client.balance().await?;
    let data = BalanceData {
        room: me.roomname.clone(),
        isbind: me.isbind,
        remaining_kwh: bal.remaining_kwh,
        subsidy_kwh: bal.subsidy_kwh,
        balance_yuan: bal.balance_yuan,
        subsidy_balance_yuan: bal.subsidy_balance_yuan,
    };
    render(Envelope::ok(data), fmt)
}

/// `sjtu elec usage [--month YYYY-MM]`：月度用电聚合。
///
/// `--month` 缺省 = 当前月（`chrono::Local`）。
pub async fn cmd_usage(month: Option<String>, fmt: Option<OutputFormat>) -> Result<()> {
    let (year, mm) = parse_month(month.as_deref())?;
    let client = Client::connect().await?;
    let m = client.monthly(year, mm).await?;
    let data = UsageData {
        year,
        month: mm,
        last_month_kwh: m.last,
        this_month_kwh: m.now,
    };
    render(Envelope::ok(data), fmt)
}

/// `sjtu elec history [--days N]`：日用电明细。
///
/// 默认 7 天，最大 90 天（守一个安全上限，超 30 天范围未真机验证）。
pub async fn cmd_history(days: u32, fmt: Option<OutputFormat>) -> Result<()> {
    if days == 0 || days > 90 {
        return Err(SjtuCliError::InvalidInput(format!("--days {days} 超出范围 (1..=90)")).into());
    }
    let end = chrono::Local::now().date_naive();
    let begin = end - Duration::days((days as i64) - 1);
    let client = Client::connect().await?;
    let items = client.history(begin, end).await?;
    let total_kwh: Decimal = items.iter().map(|it| it.used).sum();
    let data = HistoryData {
        begin: begin.format("%Y-%m-%d").to_string(),
        end: end.format("%Y-%m-%d").to_string(),
        returned: items.len(),
        total_kwh,
        items,
    };
    render(Envelope::ok(data), fmt)
}

/// 解析 `YYYY-MM` 为 `(year, month)`；`None` 时取当月。
fn parse_month(s: Option<&str>) -> Result<(u16, u8)> {
    if let Some(s) = s {
        let parts: Vec<&str> = s.splitn(2, '-').collect();
        if parts.len() != 2 {
            return Err(
                SjtuCliError::InvalidInput(format!("--month {s} 期望 YYYY-MM 格式")).into(),
            );
        }
        let y: u16 = parts[0]
            .parse()
            .map_err(|_| SjtuCliError::InvalidInput(format!("--month {s} 年份非法")))?;
        let m: u8 = parts[1]
            .parse()
            .map_err(|_| SjtuCliError::InvalidInput(format!("--month {s} 月份非法")))?;
        if !(1..=12).contains(&m) || !(2000..=2100).contains(&y) {
            return Err(SjtuCliError::InvalidInput(format!("--month {s} 月份/年份范围错")).into());
        }
        Ok((y, m))
    } else {
        let now = chrono::Local::now().date_naive();
        Ok((now.year() as u16, now.month() as u8))
    }
}
