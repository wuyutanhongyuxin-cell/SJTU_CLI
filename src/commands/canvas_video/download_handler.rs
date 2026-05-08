//! `sjtu canvas-video download` handler（CP-V3+）：单讲单/双机位下载。
//!
//! 流程：connect → list_lectures → 过滤已审 + 排序 → 取第 N 讲 → get_video_info →
//! download_to_file（Range 分片并发）→ Envelope。`--all-channels` 走双机位顺序下载，
//! 共享课程级元数据，每路一条 `ChannelOutput`。

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;

use crate::apps::canvas_video::{download::download_to_file, Client, LectureVideo, VideoFetch};
use crate::error::SjtuCliError;
use crate::output::{render, Envelope, OutputFormat};

use super::data::{ChannelOutput, DownloadAllData, DownloadData};
use super::handlers::{absolutize, redact_or_full, redact_url, safe_filename};

/// 下载 mp4 必带的 Referer（实测 SJTU CDN 不带就 403）。
const DOWNLOAD_REFERER: &str = "https://courses.sjtu.edu.cn";

/// `sjtu canvas-video download <course_id> --lecture N --to <dir>`：单讲单机位落盘。
#[allow(clippy::too_many_arguments)]
pub async fn cmd_download(
    course_id: u64,
    tool_id: u64,
    lecture: u32,
    to_dir: PathBuf,
    channel: i32,
    concurrency: usize,
    with_identity: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let started = Instant::now();
    let client = Client::connect(course_id, tool_id).await?;
    let target = resolve_target(&client, lecture).await?;
    tokio::fs::create_dir_all(&to_dir)
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("mkdir {}: {e}", to_dir.display())))?;
    let (fetch, out) = download_one_channel(
        &client,
        &target,
        channel,
        &to_dir,
        concurrency,
        with_identity,
    )
    .await?;
    render(
        Envelope::ok(DownloadData {
            course_id,
            tool_id,
            lecture,
            channel: out.channel,
            video_name: fetch.video_name.or(Some(target.video_name)),
            video_id_redacted: redact_or_full(&target.video_id, with_identity),
            duration_secs: fetch.duration_secs,
            file_path: out.file_path,
            bytes: out.bytes,
            elapsed_ms: started.elapsed().as_millis(),
            mp4_url_redacted: out.mp4_url_redacted,
        }),
        fmt,
    )
}

/// `sjtu canvas-video download <course_id> --lecture N --to <dir> --all-channels`：双机位
/// 顺序下载（channel 0 → channel 1），共享课程级元数据。
#[allow(clippy::too_many_arguments)]
pub async fn cmd_download_all(
    course_id: u64,
    tool_id: u64,
    lecture: u32,
    to_dir: PathBuf,
    concurrency: usize,
    with_identity: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let started = Instant::now();
    let client = Client::connect(course_id, tool_id).await?;
    let target = resolve_target(&client, lecture).await?;
    tokio::fs::create_dir_all(&to_dir)
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("mkdir {}: {e}", to_dir.display())))?;
    let mut channels: Vec<ChannelOutput> = Vec::with_capacity(2);
    let mut video_name: Option<String> = None;
    let mut duration_secs: Option<i64> = None;
    let mut total_bytes = 0u64;
    for ch in [0i32, 1] {
        let (fetch, out) =
            download_one_channel(&client, &target, ch, &to_dir, concurrency, with_identity).await?;
        if video_name.is_none() {
            video_name = fetch.video_name.clone();
        }
        if duration_secs.is_none() {
            duration_secs = fetch.duration_secs;
        }
        total_bytes += out.bytes;
        channels.push(out);
    }
    render(
        Envelope::ok(DownloadAllData {
            course_id,
            tool_id,
            lecture,
            video_name: video_name.or(Some(target.video_name)),
            video_id_redacted: redact_or_full(&target.video_id, with_identity),
            duration_secs,
            channels,
            total_bytes,
            total_elapsed_ms: started.elapsed().as_millis(),
        }),
        fmt,
    )
}

/// `--lecture N` → 实际 `LectureVideo`：list_lectures + 过滤 vide_audit_status==3 + 按
/// course_begin_time 升序 + nth(N-1)。
async fn resolve_target(client: &Client, lecture: u32) -> Result<LectureVideo> {
    if lecture == 0 {
        return Err(SjtuCliError::InvalidInput("--lecture 从 1 起，0 无效".into()).into());
    }
    let (raw, _) = client
        .list_lectures(client.cour_id(), client.lti_course_id())
        .await?;
    let mut audited: Vec<LectureVideo> = raw
        .into_iter()
        .filter(|v| v.vide_audit_status == Some(3))
        .collect();
    audited.sort_by(|a, b| {
        a.course_begin_time
            .as_deref()
            .unwrap_or("")
            .cmp(b.course_begin_time.as_deref().unwrap_or(""))
    });
    let total = audited.len();
    audited
        .into_iter()
        .nth(lecture as usize - 1)
        .ok_or_else(|| {
            SjtuCliError::InvalidInput(format!("课程仅 {total} 讲（已审），不存在第 {lecture} 讲"))
                .into()
        })
}

/// 单 channel 下载：get_video_info → mkfilename → download_to_file → ChannelOutput。
/// 调用方负责 mkdir。返回 `(VideoFetch, ChannelOutput)`，前者供调用方提元数据。
async fn download_one_channel(
    client: &Client,
    target: &LectureVideo,
    channel: i32,
    to_dir: &Path,
    concurrency: usize,
    with_identity: bool,
) -> Result<(VideoFetch, ChannelOutput)> {
    let started = Instant::now();
    let fetch = client.get_video_info(&target.video_id, channel).await?;
    let stem = target.video_name.as_str().trim();
    let safe_stem = safe_filename(if stem.is_empty() { "video" } else { stem });
    let filename = format!("{safe_stem}_ch{}.mp4", fetch.channel);
    let dest = to_dir.join(&filename);
    let bytes = download_to_file(&fetch.mp4_url, &dest, concurrency, DOWNLOAD_REFERER).await?;
    let out = ChannelOutput {
        channel: fetch.channel,
        file_path: absolutize(&dest),
        bytes,
        elapsed_ms: started.elapsed().as_millis(),
        mp4_url_redacted: redact_url(&fetch.mp4_url, with_identity),
    };
    Ok((fetch, out))
}
