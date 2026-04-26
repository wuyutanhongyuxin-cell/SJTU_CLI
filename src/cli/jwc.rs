//! `sjtu jwc <sub>` 教务系统命令的 clap 枚举 + 派发。
//!
//! 命令清单（MVP 仅 1 条）：
//! - `grades` —— §2.1 N305005 学生成绩查询
//!
//! 后续按 tasks/isjtu_investigation.md §2 顺序补：schedule (N2151) /
//! gpa (N309131) / exams (N358105) 等。每个 SP 一个 `JwcSub` variant。

use anyhow::Result;
use clap::Subcommand;

use crate::commands::jwc as jwc_cmds;
use crate::output::OutputFormat;

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
    }
}
