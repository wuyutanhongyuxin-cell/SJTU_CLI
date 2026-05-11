//! audio_dl orchestrator：moov 定位 + Range 合并 + 并发拉 + mux。

use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use tracing::{debug, info};

use crate::error::SjtuCliError;

/// 头部探测大小（1 MB）。SJTU CDN moov 实测最大 ~700 KB。
pub(super) const HEAD_PROBE_SIZE: u64 = 1024 * 1024;
const TAIL_PROBE_INITIAL: u64 = 1024 * 1024;
/// 尾部最大探测上限（16 MB），超出视为非常规 mp4。
pub(super) const TAIL_PROBE_MAX: u64 = 16 * 1024 * 1024;
/// chunk 间无字节流入超时（V5.B 第 9 讲事故缓解）。
pub(super) const INTER_BYTE_TIMEOUT: Duration = Duration::from_secs(30);

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
    use crate::apps::canvas_video::mp4_box::parse_moov;

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
    let ranges = super::ranges::merge_ranges(&samples, 64 * 1024);
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

    // write_m4a 同步 std::fs::write（~20 MB），async 上下文必须 spawn_blocking 避免堵塞 executor。
    let dest = dest_m4a.to_path_buf();
    let track_for_mux = track.clone();
    let written = tokio::task::spawn_blocking(move || {
        crate::apps::canvas_video::m4a_mux::write_m4a(&dest, &track_for_mux, &sample_bytes)
    })
    .await
    .map_err(|e| SjtuCliError::NetworkError(format!("write_m4a spawn_blocking 失败: {e}")))??;
    Ok(DownloadStats {
        written,
        downloaded,
    })
}

/// 探测 mp4 size 并定位 moov box，返回 (moov 字节, 已下载字节数)。
pub(super) async fn locate_moov(client: &Client, url: &str) -> Result<(Vec<u8>, u64)> {
    let total = probe_size(client, url).await?;
    if total == 0 {
        bail!("probe size=0：{url}");
    }
    let mut dl: u64 = 1; // probe 1 字节
    let head_end = (HEAD_PROBE_SIZE - 1).min(total - 1);
    let head = fetch_range(client, url, 0, head_end).await?;
    dl += head.len() as u64;
    if let Some((moov_pos, moov_size)) = scan_for_moov(&head) {
        if moov_pos as u64 + moov_size <= head.len() as u64 {
            return Ok((head[moov_pos..moov_pos + moov_size as usize].to_vec(), dl));
        }
        // moov 跨 1 MB 边界：补拉剩余部分
        let extra_start = head.len() as u64;
        let extra_end = (moov_pos as u64 + moov_size - 1).min(total - 1);
        let extra = fetch_range(client, url, extra_start, extra_end).await?;
        dl += extra.len() as u64;
        let mut full = head[moov_pos..].to_vec();
        full.extend_from_slice(&extra);
        full.truncate(moov_size as usize);
        return Ok((full, dl));
    }
    // 头部无 moov → 尾部翻倍探测
    let mut probe = TAIL_PROBE_INITIAL;
    while probe <= TAIL_PROBE_MAX {
        let tail_start = total.saturating_sub(probe);
        let tail = fetch_range(client, url, tail_start, total - 1).await?;
        dl += tail.len() as u64;
        if let Some((rel, moov_size)) = scan_for_moov(&tail) {
            if rel as u64 + moov_size <= tail.len() as u64 {
                return Ok((tail[rel..rel + moov_size as usize].to_vec(), dl));
            }
            bail!("尾部 moov 跨边界（不可能）");
        }
        probe *= 2;
    }
    bail!(
        "尾部 {} MB 仍找不到 moov，疑似非 mp4 容器",
        TAIL_PROBE_MAX / 1024 / 1024
    )
}

async fn probe_size(client: &Client, url: &str) -> Result<u64> {
    let resp = client
        .get(url)
        .header(RANGE, "bytes=0-0")
        .send()
        .await
        .map_err(neterr("probe"))?;
    let st = resp.status();
    if !st.is_success() && st != StatusCode::PARTIAL_CONTENT {
        bail!("probe status={st}");
    }
    if st == StatusCode::PARTIAL_CONTENT {
        return Ok(resp
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit('/').next())
            .and_then(|t| t.parse().ok())
            .unwrap_or(0));
    }
    Ok(resp.content_length().unwrap_or(0))
}

/// 下载 `start..=end` 字节，内嵌 30 s inter-byte 超时。
pub(super) async fn fetch_range(
    client: &Client,
    url: &str,
    start: u64,
    end: u64,
) -> Result<Vec<u8>> {
    let rv = format!("bytes={start}-{end}");
    let mut resp = client
        .get(url)
        .header(RANGE, &rv)
        .send()
        .await
        .map_err(neterr("range get"))?;
    let st = resp.status();
    if st != StatusCode::PARTIAL_CONTENT && !st.is_success() {
        bail!("段 {rv} status={st}");
    }
    let mut buf: Vec<u8> = Vec::with_capacity((end - start + 1) as usize);
    loop {
        let chunk = tokio::time::timeout(INTER_BYTE_TIMEOUT, resp.chunk())
            .await
            .map_err(|_| SjtuCliError::NetworkError(format!("段 {rv} 30s 无字节流入，abort")))?
            .map_err(neterr("chunk"))?;
        let Some(c) = chunk else { break };
        buf.extend_from_slice(&c);
    }
    debug!(start, end, len = buf.len(), "段完成");
    Ok(buf)
}

/// 扫顶层 box 找 moov，返回 (偏移, size)。
fn scan_for_moov(buf: &[u8]) -> Option<(usize, u64)> {
    let mut pos = 0usize;
    while pos + 8 <= buf.len() {
        let size32 = u32::from_be_bytes(buf[pos..pos + 4].try_into().ok()?);
        let ty: [u8; 4] = buf[pos + 4..pos + 8].try_into().ok()?;
        let size = if size32 == 1 {
            if buf.len() < pos + 16 {
                return None;
            }
            u64::from_be_bytes(buf[pos + 8..pos + 16].try_into().ok()?)
        } else if size32 == 0 {
            return None;
        } else {
            size32 as u64
        };
        if &ty == b"moov" {
            return Some((pos, size));
        }
        pos = pos.checked_add(size as usize)?;
    }
    None
}

fn neterr(ctx: &'static str) -> impl Fn(reqwest::Error) -> SjtuCliError {
    move |e| SjtuCliError::NetworkError(format!("{ctx}: {e}"))
}
