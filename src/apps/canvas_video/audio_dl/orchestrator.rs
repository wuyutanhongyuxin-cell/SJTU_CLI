//! audio_dl orchestrator：moov 定位 + Range 合并 + 并发拉 + mux。

#![allow(dead_code)] // T7/T8 才用；该 allow 在 T8 完工时删

use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Result};
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use tracing::debug;

use crate::error::SjtuCliError;

/// 头部 / 尾部 probe Range 大小，单位字节。SJTU CDN moov 实测最大 ~700 KB。
const HEAD_PROBE_SIZE: u64 = 1024 * 1024; // 1 MB
const TAIL_PROBE_INITIAL: u64 = 1024 * 1024; // 1 MB
const TAIL_PROBE_MAX: u64 = 16 * 1024 * 1024; // 16 MB（仍找不到 moov 视为非常规 mp4）
/// chunk 间无字节流入超时（V5.B Phase 1 第 9 讲事故的直接缓解）
pub(super) const INTER_BYTE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub struct DownloadStats {
    /// m4a 落盘字节数（≈ audio_track 总 sample 字节 + 容器 overhead）
    pub written: u64,
    /// 实际从 CDN 拉的字节数（≈ moov 区段 + 合并后 Range 总长）
    pub downloaded: u64,
}

pub async fn download_audio_only_to_file(
    _url: &str,
    _dest_m4a: &Path,
    _concurrency: usize,
    _referer: &str,
) -> Result<DownloadStats> {
    bail!("download_audio_only_to_file 主流程见 Task 7 / Task 8")
}

/// 仅供测试使用的 moov 定位入口。
#[cfg(test)]
pub(super) async fn locate_moov_for_test(url: &str, referer: &str) -> Result<(Vec<u8>, u64)> {
    use super::client::build_client_audio;
    let client = build_client_audio(referer)?;
    locate_moov(&client, url).await
}

/// 探测 mp4 size 并定位 moov box，返回 (moov 字节, 已下载字节数)。
pub(super) async fn locate_moov(client: &Client, url: &str) -> Result<(Vec<u8>, u64)> {
    let total = probe_size(client, url).await?;
    if total == 0 {
        bail!("probe size=0：{url}");
    }
    let mut downloaded: u64 = 1; // 包含 probe 1 字节
                                 // 1. 头部 1 MB
    let head_end = (HEAD_PROBE_SIZE - 1).min(total - 1);
    let head = fetch_range(client, url, 0, head_end).await?;
    downloaded += head.len() as u64;
    if let Some((moov_pos, moov_size)) = scan_for_moov(&head) {
        // 头部含 moov，但可能跨 1 MB 边界
        if moov_pos as u64 + moov_size <= head.len() as u64 {
            return Ok((
                head[moov_pos..moov_pos + moov_size as usize].to_vec(),
                downloaded,
            ));
        }
        // moov 跨界：补一段拿全 moov
        let extra_start = head.len() as u64;
        let extra_end = (moov_pos as u64 + moov_size - 1).min(total - 1);
        let extra = fetch_range(client, url, extra_start, extra_end).await?;
        downloaded += extra.len() as u64;
        let mut full = head[moov_pos..].to_vec();
        full.extend_from_slice(&extra);
        full.truncate(moov_size as usize);
        return Ok((full, downloaded));
    }
    // 2. 头部不含 moov → 尾部翻倍探测
    let mut probe = TAIL_PROBE_INITIAL;
    while probe <= TAIL_PROBE_MAX {
        let tail_start = total.saturating_sub(probe);
        let tail = fetch_range(client, url, tail_start, total - 1).await?;
        downloaded += tail.len() as u64;
        if let Some((rel, moov_size)) = scan_for_moov(&tail) {
            if rel as u64 + moov_size <= tail.len() as u64 {
                return Ok((tail[rel..rel + moov_size as usize].to_vec(), downloaded));
            }
            // tail 已覆盖到尾部，理论不应跨界
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
        // 30 s inter-byte timeout：tokio::time::timeout 包 chunk()
        let chunk = tokio::time::timeout(INTER_BYTE_TIMEOUT, resp.chunk())
            .await
            .map_err(|_| SjtuCliError::NetworkError(format!("段 {rv} 30s 无字节流入，abort")))?
            .map_err(neterr("chunk"))?;
        let Some(c) = chunk else {
            break;
        };
        buf.extend_from_slice(&c);
    }
    debug!(start, end, len = buf.len(), "段完成");
    Ok(buf)
}

/// 在 buf 里找 moov box，返回 (相对 buf 起点的偏移, moov size)。
/// 顺序扫顶层 box，遇到 moov 即返；若整段都不是 moov 返 None。
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
