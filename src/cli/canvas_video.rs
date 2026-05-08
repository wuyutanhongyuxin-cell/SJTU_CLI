//! `sjtu canvas-video <sub>` 的 clap 枚举 + 派发。
//!
//! 命令清单：
//! - `list <course-id> [--tool-id 8329] [--with-identity] [--include-unaudited]`
//! - `download <course-id> --lecture N --to <dir> [--channel 0] [--concurrency 8]
//!     [--tool-id 8329] [--with-identity]`（CP-V3）
//!
//! 与 `sjtu canvas`（PAT 鉴权）独立两条链。

use std::path::PathBuf;

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

    /// 下载某门课的某讲到本地（mp4，Range 分片并发）。
    Download {
        /// Canvas 数字课程 ID。
        course_id: u64,

        /// 讲序号（1-based，按 course_begin_time 升序后的位置；与 `list` 输出的 `seq` 对齐）。
        #[arg(long)]
        lecture: u32,

        /// 输出目录（不存在自动创建）。
        #[arg(long)]
        to: PathBuf,

        /// 机位：0=老师正面 / 1=PPT。默认 0（与 SPA 一致）。与 `--all-channels` 互斥。
        #[arg(long, default_value_t = 0, conflicts_with = "all_channels")]
        channel: i32,

        /// 双机位都下载（顺序 channel 0 → 1，落两个独立 mp4）。与 `--channel` 互斥。
        #[arg(long, default_value_t = false)]
        all_channels: bool,

        /// 单文件分片并发数（< 2 走单段流式）。CP-V3.1 后每段独立 TCP（HTTP/1.1 + 关池），
        /// 8 路稳定，吞吐 ~6-10 MB/s（aria2 -x 16 等效路径）。
        #[arg(long, default_value_t = 8)]
        concurrency: usize,

        /// LTI 工具 ID。默认 `8329`。
        #[arg(long, default_value_t = 8329)]
        tool_id: u64,

        /// 输出含 PII 的内部字段（mp4 URL 全文 / videoId 全文）。
        #[arg(long)]
        with_identity: bool,
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
        CanvasVideoSub::Download {
            course_id,
            lecture,
            to,
            channel,
            all_channels,
            concurrency,
            tool_id,
            with_identity,
        } => {
            if all_channels {
                cv_cmds::cmd_download_all(
                    course_id,
                    tool_id,
                    lecture,
                    to,
                    concurrency,
                    with_identity,
                    fmt,
                )
                .await
            } else {
                cv_cmds::cmd_download(
                    course_id,
                    tool_id,
                    lecture,
                    to,
                    channel,
                    concurrency,
                    with_identity,
                    fmt,
                )
                .await
            }
        }
    }
}
