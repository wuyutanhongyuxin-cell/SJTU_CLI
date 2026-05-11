//! `sjtu canvas-video download` handler（CP-V3 + CP-V4 audio-only）：单讲单/双机位下载。
//!
//! 流程：connect → list_lectures → 过滤已审 + 排序 → 取第 N 讲 → get_video_info →
//! download_to_file（Range 分片并发）→ [可选 ffmpeg -vn -c:a copy 抽 m4a + 删 mp4]
//! → Envelope。`--all-channels` 走双机位顺序下载，共享课程级元数据。

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;

use crate::output::{render, Envelope, OutputFormat};

use super::data::{ChannelOutput, DownloadAllData, DownloadData};
use super::download_shared::{download_one_channel, prep};

/// `sjtu canvas-video download <course_id> --lecture N --to <dir>`：单讲单机位落盘。
#[allow(clippy::too_many_arguments)]
pub async fn cmd_download(
    course_id: u64,
    tool_id: u64,
    lecture: u32,
    to_dir: PathBuf,
    channel: i32,
    concurrency: usize,
    audio_only: bool,
    keep_mp4: bool,
    with_identity: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let started = Instant::now();
    prep(audio_only, &to_dir).await?;

    let (fetch, out, target_video_name, target_video_id) =
        super::retry::with_token_refresh(course_id, tool_id, |client| {
            // to_dir 是 PathBuf（非 Copy），Fn 闭包多次调用时每次 clone 一份进 async move
            let to_dir = to_dir.clone();
            async move {
                let target = super::handlers::resolve_target(&client, lecture).await?;
                // video_name / video_id 要 clone 穿越 await 边界，target 在闭包结束时 drop
                let target_video_name = target.video_name.clone();
                let target_video_id = target.video_id.clone();
                let (fetch, out) = download_one_channel(
                    &client,
                    &target,
                    channel,
                    &to_dir,
                    concurrency,
                    audio_only,
                    keep_mp4,
                    with_identity,
                )
                .await?;
                Ok::<_, anyhow::Error>((fetch, out, target_video_name, target_video_id))
            }
        })
        .await?;

    render(
        Envelope::ok(DownloadData {
            course_id,
            tool_id,
            lecture,
            channel: out.channel,
            video_name: fetch.video_name.or(Some(target_video_name)),
            video_id_redacted: super::handlers::redact_or_full(&target_video_id, with_identity),
            duration_secs: fetch.duration_secs,
            file_path: out.file_path,
            audio_path: out.audio_path,
            mp4_kept: out.mp4_kept,
            bytes: out.bytes,
            elapsed_ms: started.elapsed().as_millis(),
            mp4_url_redacted: out.mp4_url_redacted,
            download_kind: out.download_kind.clone(),
            bytes_downloaded: out.bytes_downloaded,
        }),
        fmt,
    )
}

/// `sjtu canvas-video download <course_id> --lecture N --to <dir> --all-channels`：双机位
/// 顺序下载（channel 0 → channel 1），共享课程级元数据。
/// ch1 中途 token 失效时，整套（含 ch0）回到闭包头重跑；spec 接受该边界。
#[allow(clippy::too_many_arguments)]
pub async fn cmd_download_all(
    course_id: u64,
    tool_id: u64,
    lecture: u32,
    to_dir: PathBuf,
    concurrency: usize,
    audio_only: bool,
    keep_mp4: bool,
    with_identity: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let started = Instant::now();
    prep(audio_only, &to_dir).await?;

    let (channels, video_name, duration_secs, total_bytes, target_video_name, target_video_id) =
        super::retry::with_token_refresh(course_id, tool_id, |client| {
            let to_dir = to_dir.clone();
            async move {
                let target = super::handlers::resolve_target(&client, lecture).await?;
                let target_video_name = target.video_name.clone();
                let target_video_id = target.video_id.clone();
                let mut channels: Vec<ChannelOutput> = Vec::with_capacity(2);
                let mut video_name: Option<String> = None;
                let mut duration_secs: Option<i64> = None;
                let mut total_bytes = 0u64;
                for ch in [0i32, 1] {
                    let (fetch, out) = download_one_channel(
                        &client,
                        &target,
                        ch,
                        &to_dir,
                        concurrency,
                        audio_only,
                        keep_mp4,
                        with_identity,
                    )
                    .await?;
                    if video_name.is_none() {
                        video_name = fetch.video_name.clone();
                    }
                    if duration_secs.is_none() {
                        duration_secs = fetch.duration_secs;
                    }
                    total_bytes += out.bytes;
                    channels.push(out);
                }
                Ok::<_, anyhow::Error>((
                    channels,
                    video_name,
                    duration_secs,
                    total_bytes,
                    target_video_name,
                    target_video_id,
                ))
            }
        })
        .await?;

    render(
        Envelope::ok(DownloadAllData {
            course_id,
            tool_id,
            lecture,
            video_name: video_name.or(Some(target_video_name)),
            video_id_redacted: super::handlers::redact_or_full(&target_video_id, with_identity),
            duration_secs,
            channels,
            total_bytes,
            total_elapsed_ms: started.elapsed().as_millis(),
        }),
        fmt,
    )
}
