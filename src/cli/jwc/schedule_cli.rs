//! `sjtu jwc today / week / next` 三个衍生命令的 args struct（拆出来守 200 行硬限）。

use clap::Args;

/// `sjtu jwc today [--xnm 2025] [--xqm 12] [--grid]`：今日剩余的课。
#[derive(Debug, Args)]
pub struct TodayArgs {
    /// 学年 4 位（如 `2025`）。留空 = 当前学年。
    #[arg(long)]
    pub xnm: Option<String>,
    /// 学期编码：`3`=秋季 / `12`=春季 / `16`=夏季。留空 = 当前学期。
    #[arg(long)]
    pub xqm: Option<String>,
    /// 用 1×N 网格形式输出（仅 TTY；非 TTY 时 fallback YAML）。
    #[arg(long, default_value_t = false)]
    pub grid: bool,
}

/// `sjtu jwc week [--zs N] [--xnm 2025] [--xqm 12] [--grid]`：整周课表。
#[derive(Debug, Args)]
pub struct WeekArgs {
    /// 学年 4 位。
    #[arg(long)]
    pub xnm: Option<String>,
    /// 学期编码。
    #[arg(long)]
    pub xqm: Option<String>,
    /// 教学周次 1..=18。留空 = 自动反推当前周。
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=18))]
    pub zs: Option<u8>,
    /// 用 7×N 网格形式输出。
    #[arg(long, default_value_t = false)]
    pub grid: bool,
}

/// `sjtu jwc next [--within N] [--limit K]`：接下来若干天的前 K 节课。
#[derive(Debug, Args)]
pub struct NextArgs {
    #[arg(long)]
    pub xnm: Option<String>,
    #[arg(long)]
    pub xqm: Option<String>,
    /// 时间窗口：未来 N 天内的课（1..=31）。默认 1（仅今天剩余）。
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=31), default_value_t = 1)]
    pub within: u8,
    /// 最多返回前 K 节课。默认 5。
    #[arg(long, default_value_t = 5)]
    pub limit: u8,
}
