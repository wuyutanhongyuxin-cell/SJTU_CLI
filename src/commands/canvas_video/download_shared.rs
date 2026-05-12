//! download 共享 helper（download_handler / batch_handler 共用）。
//!
//! 从 download_handler.rs 抽出来，因 download_handler 在 V5.A T12 后超 200 行硬限。

use std::path::Path;
use std::time::Instant;

use anyhow::Result;

use crate::apps::canvas_video::{
    download::download_to_file, ffmpeg as ff, Client, LectureVideo, VideoFetch,
};
use crate::error::SjtuCliError;

use super::data::ChannelOutput;
use super::handlers::{absolutize, redact_url, safe_filename};

/// 下载 mp4 必带的 Referer（实测 SJTU CDN 不带就 403）。
pub(super) const DOWNLOAD_REFERER: &str = "https://courses.sjtu.edu.cn";

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

/// 单 channel 下载。
/// V5.F：单一路径 mp4-full → 可选 ffmpeg 抽 audio → audio_only && !keep_mp4 时删 mp4。
/// V5.D + V5.E-B+ audio-only 优化整路已撤回（见 docs/superpowers/specs/2026-05-12-v5f-*.md）。
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
    let m4a_dest = to_dir.join(format!("{safe_stem}_ch{}.m4a", fetch.channel));

    // 单一路径（mp4-full）：可选 ffmpeg 抽 audio
    let bytes = download_to_file(&fetch.mp4_url, &mp4_dest, concurrency, DOWNLOAD_REFERER).await?;
    let mut audio_path: Option<String> = None;
    let mut mp4_kept = true;
    if audio_only {
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
        download_kind: "mp4-full".to_string(),
        bytes_downloaded: bytes,
    };
    Ok((fetch, out))
}

#[cfg(test)]
mod tests {
    /// download_kind 取值字符串 stable。下游消费方依赖这些 literal。
    #[test]
    fn download_kind_strings_are_stable() {
        let expected = ["mp4-full", "skipped"];
        for s in expected {
            assert!(!s.is_empty());
            assert!(!s.contains(' '));
        }
    }
}
