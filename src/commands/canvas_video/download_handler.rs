//! `sjtu canvas-video download` handler（CP-V3 + CP-V4 audio-only）：单讲单/双机位下载。
//!
//! 流程：connect → list_lectures → 过滤已审 + 排序 → 取第 N 讲 → get_video_info →
//! download_to_file（Range 分片并发）→ [可选 ffmpeg -vn -c:a copy 抽 m4a + 删 mp4]
//! → Envelope。`--all-channels` 走双机位顺序下载，共享课程级元数据。

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;

use crate::apps::canvas_video::{
    download::download_to_file, ffmpeg as ff, Client, LectureVideo, VideoFetch,
};
use crate::error::SjtuCliError;
use crate::output::{render, Envelope, OutputFormat};

use super::data::{ChannelOutput, DownloadAllData, DownloadData};
use super::handlers::{absolutize, redact_url, safe_filename};

/// 下载 mp4 必带的 Referer（实测 SJTU CDN 不带就 403）。
const DOWNLOAD_REFERER: &str = "https://courses.sjtu.edu.cn";

/// 三个 cmd_* 共享：audio_only 时早爆 ffmpeg 缺失，连 client 都不建；mkdir 输出目录。
pub(super) async fn prep(audio_only: bool, to_dir: &Path) -> Result<()> {
    if audio_only {
        ff::ensure_ffmpeg()?;
    }
    tokio::fs::create_dir_all(to_dir)
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("mkdir {}: {e}", to_dir.display())))?;
    Ok(())
}

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

/// 单 channel 下载：get_video_info → mkfilename → download_to_file → 可选 ffmpeg 抽 m4a
/// → 可选删 mp4 → ChannelOutput。批量 handler 也复用该函数。
#[allow(clippy::too_many_arguments)]
pub(super) async fn download_one_channel(
    client: &Client,
    target: &LectureVideo,
    channel: i32,
    to_dir: &Path,
    concurrency: usize,
    audio_only: bool,
    keep_mp4: bool,
    with_identity: bool,
) -> Result<(VideoFetch, ChannelOutput)> {
    let started = Instant::now();
    let fetch = client.get_video_info(&target.video_id, channel).await?;
    let stem = target.video_name.as_str().trim();
    let safe_stem = safe_filename(if stem.is_empty() { "video" } else { stem });
    let mp4_dest = to_dir.join(format!("{safe_stem}_ch{}.mp4", fetch.channel));
    let bytes = download_to_file(&fetch.mp4_url, &mp4_dest, concurrency, DOWNLOAD_REFERER).await?;
    let mut audio_path: Option<String> = None;
    let mut mp4_kept = true;
    if audio_only {
        let m4a_dest = to_dir.join(format!("{safe_stem}_ch{}.m4a", fetch.channel));
        ff::extract_audio(&mp4_dest, &m4a_dest).await?;
        audio_path = Some(absolutize(&m4a_dest));
        if !keep_mp4 {
            tokio::fs::remove_file(&mp4_dest).await.ok();
            mp4_kept = false;
        }
    }
    let out = ChannelOutput {
        channel: fetch.channel,
        file_path: absolutize(&mp4_dest),
        audio_path,
        mp4_kept,
        bytes,
        elapsed_ms: started.elapsed().as_millis(),
        mp4_url_redacted: redact_url(&fetch.mp4_url, with_identity),
    };
    Ok((fetch, out))
}
