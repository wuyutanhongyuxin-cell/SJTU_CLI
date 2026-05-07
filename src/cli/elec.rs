//! `sjtu elec <sub>` 相关的 clap 枚举 + 派发。
//!
//! MVP 三件套（均**只读**，不点任何"立即支付"按钮）：
//! - `balance` —— 当前绑定房间的余额
//! - `usage [--month YYYY-MM]` —— 月度用电聚合（上月+本月）
//! - `history [--days N]` —— 日用电明细（默认 7 天）
//!
//! Phase-2：一卡通余额 / 消费明细（需校园网 + ecard.sjtu.edu.cn 现场抓）。

use anyhow::Result;
use clap::Subcommand;

use crate::commands::elec as elec_cmds;
use crate::output::OutputFormat;

/// `sjtu elec <sub>` 的子命令集合。
#[derive(Debug, Subcommand)]
pub enum ElecSub {
    /// 当前绑定房间的电费余额。**只读**。
    ///
    /// 输出 4 个 Decimal 字段：剩余度数 / 剩余补助度数 / 剩余金额 / 剩余补助金额。
    Balance,

    /// 月度用电聚合（上月 + 当月度数）。**只读**。
    Usage {
        /// 月份，`YYYY-MM` 格式；缺省 = 当月。
        #[arg(long)]
        month: Option<String>,
    },

    /// 日用电明细。**只读**。
    History {
        /// 天数，默认 7，最大 90。
        #[arg(long, default_value_t = 7)]
        days: u32,
    },
}

/// 派发 `sjtu elec <sub>` 到 `commands::elec` handler。
pub async fn dispatch(sub: ElecSub, fmt: Option<OutputFormat>) -> Result<()> {
    match sub {
        ElecSub::Balance => elec_cmds::cmd_balance(fmt).await,
        ElecSub::Usage { month } => elec_cmds::cmd_usage(month, fmt).await,
        ElecSub::History { days } => elec_cmds::cmd_history(days, fmt).await,
    }
}
