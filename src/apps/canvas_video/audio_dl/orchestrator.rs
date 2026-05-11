//! audio_dl orchestrator：moov 定位 + Dynamic P85 gap + 4-Client H2 池并发拉 + mux。
//!
//! V5.E-B+ 升级：V5.D 单 Client + http1_only + 硬编码 64 KB gap →
//! V5.E-B+ 4-Client H2 池 + Dynamic P85（`effective_gap_threshold`）。
//!
//! moov 定位 / 字节范围抓取已拆到 `super::locate` 模块。本文件只负责把
//! "定位 → 解析 → gap 计算 → 合 Range → 并发拉 → reassemble → mux" 这条主管线串起来。

use std::path::Path;

use anyhow::{Context, Result};
use tracing::info;

use crate::apps::canvas_video::mp4_box::parse_moov;

// 把 locate 模块里给同包其他文件用的符号 re-export 出来，
// 这样 fetch.rs / test_helpers.rs 原来 `super::orchestrator::fetch_range` 的导入路径不用动。
pub(super) use super::locate::{fetch_range, locate_moov};

/// 选定 merge_ranges 的 gap_threshold（bytes）。
///
/// 优先级：
/// 1. `SJTU_GAP_THRESHOLD_KB` env override（u32，调研期强制固定值）
/// 2. `super::ranges::compute_p85_gap(samples)`（Dynamic P85，V5.E-B+ 主路径）
///
/// invalid env value（非数字 / 越界）→ 走 P85 fallback。
fn effective_gap_threshold(samples: &[(u64, u32)]) -> u64 {
    if let Some(kb) = std::env::var("SJTU_GAP_THRESHOLD_KB")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
    {
        return (kb as u64) * 1024;
    }
    super::ranges::compute_p85_gap(samples)
}

#[derive(Debug)]
pub struct DownloadStats {
    /// m4a 落盘字节数
    pub written: u64,
    /// 实际从 CDN 拉的字节数
    pub downloaded: u64,
}

/// audio-only 下载主流程：build pool → locate_moov → parse_moov → effective_gap_threshold
/// → merge_ranges → parallel_ranges_pool → reassemble → mux。
pub async fn download_audio_only_to_file(
    url: &str,
    dest_m4a: &Path,
    concurrency: usize,
    referer: &str,
) -> Result<DownloadStats> {
    let pool = super::client::build_client_pool_audio(referer)?;
    info!(pool_size = pool.len(), "4-Client H2 池构建完成");
    let (moov_bytes, mut downloaded) = locate_moov(&pool[0], url).await?;
    info!(moov_size = moov_bytes.len(), "moov 定位完成");
    let track = parse_moov(&moov_bytes).with_context(|| "parse moov（fail-soft 由调用方处理）")?;
    let total_sample_bytes: u64 = track.sample_sizes.iter().map(|&s| s as u64).sum();
    info!(
        codec = %track.codec,
        sample_count = track.sample_sizes.len(),
        total_sample_bytes,
        "audio track 解析完成"
    );
    let samples: Vec<(u64, u32)> = track
        .sample_offsets
        .iter()
        .copied()
        .zip(track.sample_sizes.iter().copied())
        .collect();
    let gap = effective_gap_threshold(&samples);
    info!(
        gap_threshold_bytes = gap,
        "Dynamic P85 (or env override) 选定"
    );
    let ranges = super::ranges::merge_ranges(&samples, gap);
    info!(
        range_count = ranges.len(),
        sample_count = samples.len(),
        "Range 合并完成"
    );
    let n = concurrency.max(1).min(ranges.len().max(1));
    let fetched = super::fetch::parallel_ranges_pool(&pool, url, &ranges, n).await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// env race 保护锁（同 client.rs / ranges.rs 模式）。
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn gap_env_override_valid() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("SJTU_GAP_THRESHOLD_KB", "16");
        let samples = vec![(0u64, 1u32), (100_000, 1)];
        assert_eq!(effective_gap_threshold(&samples), 16 * 1024);
        std::env::remove_var("SJTU_GAP_THRESHOLD_KB");
    }

    #[test]
    fn gap_env_invalid_falls_back_to_p85() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("SJTU_GAP_THRESHOLD_KB", "not_a_number");
        // 1 sample → compute_p85_gap 返 P85_DEFAULT = 64 KB
        let samples = vec![(0u64, 1u32)];
        assert_eq!(effective_gap_threshold(&samples), 64 * 1024);
        std::env::remove_var("SJTU_GAP_THRESHOLD_KB");
    }
}
