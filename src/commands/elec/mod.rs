//! `sjtu elec <sub>` 子命令实现。
//!
//! MVP 命令清单（三件套，均**只读**）：
//! - `balance` —— 当前绑定房间的余额（4 个 Decimal 字段）
//! - `usage [--month YYYY-MM]` —— 月度用电聚合（上月+本月）
//! - `history [--days N]` —— 日用电明细（默认 7 天，最大 90 天）
//!
//! 端点契约：tasks/isjtu_investigation.md §7.2 + §7.3-7.5。

mod data;
mod handlers;

pub use handlers::{cmd_balance, cmd_history, cmd_usage};
