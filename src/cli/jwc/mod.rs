//! `sjtu jwc <sub>` 教务系统命令的 clap 枚举 + 派发（拆分入口）。
//!
//! 命令清单（按 §2 顺序）：
//! - `grades`   — §2.1 N305005 学生成绩查询
//! - `schedule` — §2.2 N2151 个人课表查询
//! - `gpa`      — §2.3 N309131 GPA / 学积分查询（**两阶段**）
//! - `exams`    — §2.4 N358105 考试信息查询
//! - `today`    — 今日剩余的课（自动反推当前周）
//! - `week`     — 整周课表（默认当前周，可 --zs N 指定）
//! - `next`     — 接下来若干天的课
//!
//! `Today/Week/Next` 三个 variant 的字段定义在 `schedule_cli.rs`。

use anyhow::Result;
use clap::{Subcommand, ValueEnum};

use crate::apps::jwc::{GpaRank, GpaScope};
use crate::commands::jwc as jwc_cmds;
use crate::output::OutputFormat;

mod schedule_cli;
pub use schedule_cli::{NextArgs, TodayArgs, WeekArgs};

/// `sjtu jwc gpa --scope` 的 ValueEnum。
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum GpaScopeArg {
    /// 核心课程（默认）。
    Hxkc,
    /// 全部课程。
    Qbkc,
}

impl From<GpaScopeArg> for GpaScope {
    fn from(s: GpaScopeArg) -> Self {
        match s {
            GpaScopeArg::Hxkc => GpaScope::HxKc,
            GpaScopeArg::Qbkc => GpaScope::QbKc,
        }
    }
}

/// `sjtu jwc gpa --rank` 的 ValueEnum。
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum GpaRankArg {
    /// 年级专业（默认）。
    Njzy,
    /// 年级。
    Nj,
    /// 班级。
    Bj,
}

impl From<GpaRankArg> for GpaRank {
    fn from(r: GpaRankArg) -> Self {
        match r {
            GpaRankArg::Njzy => GpaRank::NjZy,
            GpaRankArg::Nj => GpaRank::Nj,
            GpaRankArg::Bj => GpaRank::Bj,
        }
    }
}

/// `sjtu jwc <sub>` 子命令集合。
#[derive(Debug, Subcommand)]
pub enum JwcSub {
    /// 查询成绩（N305005）。`--xnm`/`--xqm` 留空 = 查全部。
    Grades {
        /// 学年 4 位（如 `2025` = 2025-2026 学年）。留空 = 全部学年。
        #[arg(long)]
        xnm: Option<String>,

        /// 学期编码：`3`=秋季 / `12`=春季 / `16`=夏季。留空 = 全部学期。
        #[arg(long)]
        xqm: Option<String>,

        /// 页码，从 1 起。
        #[arg(long, default_value_t = 1)]
        page: u32,

        /// 每页条数（ZF 允许 15..500；默认 50 足够覆盖单学期）。
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },

    /// 查询课表（N2151，学年学期视图）。`--xnm`/`--xqm` 留空 = 当前学年/学期。
    Schedule {
        /// 学年 4 位。
        #[arg(long)]
        xnm: Option<String>,
        /// 学期编码：`3`/`12`/`16`。
        #[arg(long)]
        xqm: Option<String>,
    },

    /// 查询 GPA / 学积分（N309131，**两阶段**触发统计 + 拉结果）。
    Gpa {
        /// 课程范围：`hxkc` 核心课 / `qbkc` 全部课。
        #[arg(long, value_enum, default_value_t = GpaScopeArg::Hxkc)]
        scope: GpaScopeArg,

        /// 排名范围：`njzy` 年级专业 / `nj` 年级 / `bj` 班级。
        #[arg(long, value_enum, default_value_t = GpaRankArg::Njzy)]
        rank: GpaRankArg,

        /// 起始学年学期 6 位编码（如 `202503` = 2025-2026 第 1 学期）。留空 = 累计起点。
        #[arg(long)]
        from: Option<String>,

        /// 截止学年学期 6 位编码。留空 = 累计至今。
        #[arg(long)]
        to: Option<String>,
    },

    /// 多学期 GPA 对比：客户端循环 N×3 学期 N309131，聚合输出（含 parsed 排名）。
    GpaBySemester {
        /// 课程范围：`hxkc` 核心课 / `qbkc` 全部课。
        #[arg(long, value_enum, default_value_t = GpaScopeArg::Hxkc)]
        scope: GpaScopeArg,

        /// 排名范围：`njzy` 年级专业 / `nj` 年级 / `bj` 班级。
        #[arg(long, value_enum, default_value_t = GpaRankArg::Njzy)]
        rank: GpaRankArg,

        /// 起始学年 4 位（默认当年 - 3，覆盖 4 年制本科；非 4 年学制需手给）。
        #[arg(long)]
        xnm_from: Option<u32>,

        /// 截止学年 4 位（默认当年）。
        #[arg(long)]
        xnm_to: Option<u32>,
    },

    /// 查询考试信息（N358105）。`--xnm`/`--xqm` 留空 = 当前学年/学期。
    Exams {
        /// 学年 4 位。
        #[arg(long)]
        xnm: Option<String>,
        /// 学期编码：`3`/`12`/`16`。
        #[arg(long)]
        xqm: Option<String>,

        /// 页码，从 1 起。
        #[arg(long, default_value_t = 1)]
        page: u32,
        /// 每页条数（ZF 允许 15..500；默认 50）。
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },

    /// 今日剩余的课（自动反推当前周）。
    Today(TodayArgs),
    /// 整周课表（默认当前周，可 --zs N 指定）。
    Week(WeekArgs),
    /// 接下来若干天的课（默认 within=1 = 今天剩余）。
    Next(NextArgs),
}

/// 派发 `sjtu jwc <sub>` 到 `commands::jwc` 的 handler。
pub async fn dispatch(sub: JwcSub, fmt: Option<OutputFormat>) -> Result<()> {
    match sub {
        JwcSub::Grades {
            xnm,
            xqm,
            page,
            limit,
        } => jwc_cmds::cmd_grades(xnm, xqm, page, limit, fmt).await,
        JwcSub::Schedule { xnm, xqm } => jwc_cmds::cmd_schedule(xnm, xqm, fmt).await,
        JwcSub::Gpa {
            scope,
            rank,
            from,
            to,
        } => jwc_cmds::cmd_gpa(scope.into(), rank.into(), from, to, fmt).await,
        JwcSub::GpaBySemester {
            scope,
            rank,
            xnm_from,
            xnm_to,
        } => jwc_cmds::cmd_gpa_by_semester(scope.into(), rank.into(), xnm_from, xnm_to, fmt).await,
        JwcSub::Exams {
            xnm,
            xqm,
            page,
            limit,
        } => jwc_cmds::cmd_exams(xnm, xqm, page, limit, fmt).await,
        JwcSub::Today(a) => jwc_cmds::cmd_today(a.xnm, a.xqm, a.grid, fmt).await,
        JwcSub::Week(a) => jwc_cmds::cmd_week(a.xnm, a.xqm, a.zs, a.grid, fmt).await,
        JwcSub::Next(a) => jwc_cmds::cmd_next(a.xnm, a.xqm, a.within, a.limit, fmt).await,
    }
}
