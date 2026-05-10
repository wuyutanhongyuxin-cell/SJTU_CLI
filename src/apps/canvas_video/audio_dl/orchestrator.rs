//! audio_dl orchestrator：moov 定位 + Range 合并 + 并发拉 + mux。
//! 实装见 Task 6 / Task 7 / Task 8。

use std::path::Path;

use anyhow::Result;

/// audio-only 下载结果统计。
#[allow(dead_code)] // T6+ 实装后移除
#[derive(Debug)]
pub struct DownloadStats {
    /// m4a 落盘字节数（≈ audio_track 总 sample 字节 + 容器 overhead）
    pub written: u64,
    /// 实际从 CDN 拉的字节数（≈ moov 区段 + 合并后 Range 总长）
    pub downloaded: u64,
}

/// audio-only 下载主入口。
pub async fn download_audio_only_to_file(
    _url: &str,
    _dest_m4a: &Path,
    _concurrency: usize,
    _referer: &str,
) -> Result<DownloadStats> {
    anyhow::bail!("download_audio_only_to_file 未实装（Task 6+）")
}
