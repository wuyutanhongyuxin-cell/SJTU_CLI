//! `sjtu canvas-video <sub>` 的 clap 枚举 + 派发。
//!
//! 命令清单（CP-V1 仅 `list`；CP-V3/V4 才追加 `download`）：
//! - `list <course-id> [--tool-id 8329] [--with-identity] [--include-unaudited]`
//!
//! 与现有 `sjtu canvas` 命令树独立（PAT 鉴权 vs LTI 鉴权完全两条链）。

use anyhow::Result;
use clap::Subcommand;

use crate::commands::canvas_video as cv_cmds;
use crate::output::OutputFormat;

/// `sjtu canvas-video <sub>` 的子命令集合。
#[derive(Debug, Subcommand)]
pub enum CanvasVideoSub {
    /// 列出某门课的所有课堂视频（按调研契约 `findVodVideoList` 一次取 size=2000）。
    List {
        /// Canvas 数字课程 ID（来自 `sjtu canvas` 的 PAT 路径或浏览器 URL）。
        course_id: u64,

        /// LTI 工具 ID。默认 `8329`（"课堂视频new"）。后续若 SJTU 改换工具 ID，可手动覆写。
        #[arg(long, default_value_t = 8329)]
        tool_id: u64,

        /// 输出含 PII 的内部字段（cour_id / lti_course_id / 教师 user_name 全文）。
        ///
        /// 默认模式下：教师姓名保留（教学公开属性）；cour_id/lti_course_id 抹成前缀 `***`。
        #[arg(long)]
        with_identity: bool,

        /// 包含未审核（`videAuditStatus != 3`）的视频条目。默认只显示已审核（=3）。
        #[arg(long)]
        include_unaudited: bool,
    },
}

/// 派发 `sjtu canvas-video <sub>` 到 `commands::canvas_video` 的 handler。
pub async fn dispatch(sub: CanvasVideoSub, fmt: Option<OutputFormat>) -> Result<()> {
    match sub {
        CanvasVideoSub::List {
            course_id,
            tool_id,
            with_identity,
            include_unaudited,
        } => cv_cmds::cmd_list(course_id, tool_id, with_identity, include_unaudited, fmt).await,
    }
}
