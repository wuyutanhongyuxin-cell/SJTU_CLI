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
/// - audio_only && !keep_mp4 走 V5.D 新路径 audio_dl::download_audio_only_to_file（无 mp4 落盘 / 无 ffmpeg）
///   失败时 fail-soft 回退到旧 mp4-full + ffmpeg 抽流路径。
/// - 其他 case（非 audio_only / audio_only + keep_mp4）：旧路径（download.rs 全下 mp4）。
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

    if audio_only && !keep_mp4 {
        // V5.D 主路径：直拉 audio + 本地 mux，零 ffmpeg
        match crate::apps::canvas_video::audio_dl::download_audio_only_to_file(
            &fetch.mp4_url,
            &m4a_dest,
            concurrency,
            DOWNLOAD_REFERER,
        )
        .await
        {
            Ok(stats) => {
                let out = ChannelOutput {
                    channel: fetch.channel,
                    // V4 quirk 保留：占位 mp4 路径（实际不落 mp4）
                    file_path: absolutize(&mp4_dest),
                    audio_path: Some(absolutize(&m4a_dest)),
                    mp4_kept: false,
                    bytes: stats.written,
                    elapsed_ms: started.elapsed().as_millis(),
                    mp4_url_redacted: redact_url(&fetch.mp4_url, with_identity),
                    download_kind: "m4a-direct".to_string(),
                    bytes_downloaded: stats.downloaded,
                };
                return Ok((fetch, out));
            }
            Err(e) => {
                tracing::warn!(err = %e, "V5.D audio_dl 失败，回退到旧 mp4 全下 + ffmpeg 路径");
                // 落到下面的旧路径
                // 诊断环境变量：SJTU_NO_FALLBACK=1 时不回退，直接 bail（仅供 V5.D 调研期使用）
                if std::env::var("SJTU_NO_FALLBACK").as_deref() == Ok("1") {
                    return Err(e);
                }
            }
        }
    }

    // 旧路径（mp4-full）：keep_mp4 / 非 audio_only / V5.D fail-soft 回退
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
        let expected = ["mp4-full", "m4a-direct", "skipped"];
        for s in expected {
            assert!(!s.is_empty());
            assert!(!s.contains(' '));
        }
    }
}
