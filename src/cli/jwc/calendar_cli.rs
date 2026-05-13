//! `sjtu jwc calendar` clap variant 参数。

use clap::Args;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct CalendarArgs {
    /// 学年码（如 `2025`），默认按今天日期推。
    #[arg(long)]
    pub xnm: Option<String>,

    /// 学期码（3=秋 / 12=春 / 16=夏），默认按今天日期推。
    #[arg(long)]
    pub xqm: Option<String>,

    /// 输出文件路径（不传则 .ics 写 stdout）。
    #[arg(long)]
    pub to: Option<PathBuf>,

    /// 跳过学年校历那路（不含节假日整天事件）。
    #[arg(long, default_value_t = false)]
    pub no_academic: bool,

    /// 跳过考试那路（不含 N358105 考试事件）。
    #[arg(long, default_value_t = false)]
    pub no_exams: bool,
}
