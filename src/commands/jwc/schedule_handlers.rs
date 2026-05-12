//! T7 衍生命令 handler（cmd_today / cmd_week / cmd_next）。
//!
//! **占位实现** — 真正逻辑在 T9 完成。这里仅返回 unimplemented! 让 cli 编译过。

use anyhow::Result;

use crate::output::OutputFormat;

/// 今日剩余的课（占位；T9 实装）。
pub async fn cmd_today(
    _xnm: Option<String>,
    _xqm: Option<String>,
    _grid: bool,
    _fmt: Option<OutputFormat>,
) -> Result<()> {
    unimplemented!("T9 will implement this")
}

/// 整周课表（占位；T9 实装）。
pub async fn cmd_week(
    _xnm: Option<String>,
    _xqm: Option<String>,
    _zs: Option<u8>,
    _grid: bool,
    _fmt: Option<OutputFormat>,
) -> Result<()> {
    unimplemented!("T9 will implement this")
}

/// 接下来若干天的课（占位；T9 实装）。
pub async fn cmd_next(
    _xnm: Option<String>,
    _xqm: Option<String>,
    _within: u8,
    _limit: u8,
    _fmt: Option<OutputFormat>,
) -> Result<()> {
    unimplemented!("T9 will implement this")
}
