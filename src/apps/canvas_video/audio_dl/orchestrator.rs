//! audio_dl orchestrator：moov 定位 + Range 合并 + 并发拉 + mux。
//!
//! moov 定位 / 字节范围抓取已拆到 `super::locate` 模块。本文件只负责把
//! "定位 → 解析 → 合 Range → 并发拉 → reassemble → mux" 这条主管线串起来。

use std::path::Path;

use anyhow::{Context, Result};
use tracing::info;

use crate::apps::canvas_video::mp4_box::parse_moov;

// 把 locate 模块里给同包其他文件用的符号 re-export 出来，
// 这样 fetch.rs / test_helpers.rs 原来 `super::orchestrator::fetch_range` 的导入路径不用动。
pub(super) use super::locate::{fetch_range, locate_moov};

/// merge_ranges gap_threshold。
///
/// V5.D L10 真机实测：SJTU mp4 是 **per-sample chunk** 布局（每 audio sample 一个独立
/// chunk，stco 列 56280 个 chunk offset），相邻 audio sample 在 mdat 内被 video frame
/// 分隔（间隔常 < 64 KB）。
///
/// - gap_threshold=0: 不合并 → 55699 Range，22 MB 精确下载，但 55699 个 HTTP request
///   × concurrency=8 不可行（SJTU CDN RTT/setup 主导 → 几小时）
/// - gap_threshold=64 KB: 跨 video frame 合并 → 1201 Range，6.5 min 完成 / 705 MB 下载
///   （含 video 字节但相对 mp4-full 916 MB 节省 23%，相对 baseline 21 min 加速 3.2×）
///
/// 这里取 64 KB 工程妥协。真正解决 audio-only 精确下载需要 chunk-level Range（V5.E TODO）。
const RANGE_GAP_THRESHOLD: u64 = 64 * 1024;

#[derive(Debug)]
pub struct DownloadStats {
    /// m4a 落盘字节数
    pub written: u64,
    /// 实际从 CDN 拉的字节数
    pub downloaded: u64,
}

/// audio-only 下载主流程：locate_moov → parse_moov → merge_ranges → 并发 Range → reassemble → mux。
pub async fn download_audio_only_to_file(
    url: &str,
    dest_m4a: &Path,
    concurrency: usize,
    referer: &str,
) -> Result<DownloadStats> {
    let client = super::client::build_client_audio(referer)?;
    let (moov_bytes, mut downloaded) = locate_moov(&client, url).await?;
    info!(moov_size = moov_bytes.len(), "moov 定位完成");
    let track = parse_moov(&moov_bytes).with_context(|| "parse moov（fail-soft 由调用方处理）")?;
    let total_sample_bytes: u64 = track.sample_sizes.iter().map(|&s| s as u64).sum();
    info!(codec = %track.codec, sample_count = track.sample_sizes.len(),
          total_sample_bytes, "audio track 解析完成");
    let samples: Vec<(u64, u32)> = track
        .sample_offsets
        .iter()
        .copied()
        .zip(track.sample_sizes.iter().copied())
        .collect();
    let ranges = super::ranges::merge_ranges(&samples, RANGE_GAP_THRESHOLD);
    info!(
        range_count = ranges.len(),
        sample_count = samples.len(),
        "Range 合并完成"
    );
    let n = concurrency.max(1).min(ranges.len().max(1));
    let fetched = super::fetch::parallel_ranges(&client, url, &ranges, n).await?;
    let fetched_bytes: u64 = fetched.iter().map(|(_, b)| b.len() as u64).sum();
    downloaded += fetched_bytes;
    info!(fetched_bytes, "所有 Range 拉取完成");
    let sample_bytes = super::fetch::reassemble_samples(&track, &ranges, &fetched)?;
    debug_assert_eq!(sample_bytes.len() as u64, total_sample_bytes);
    let written = crate::apps::canvas_video::m4a_mux::write_m4a_async(
        dest_m4a.to_path_buf(),
        track.clone(),
        sample_bytes,
    )
    .await?;
    Ok(DownloadStats {
        written,
        downloaded,
    })
}
