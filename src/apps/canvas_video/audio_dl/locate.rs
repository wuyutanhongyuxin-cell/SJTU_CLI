//! moov 定位 + Range 探测：从 mp4 远端拉最少字节定位 moov box。
//!
//! V5.D bug 修复：SJTU CDN mp4 真实布局是 `[ftyp][mdat 99%][moov 2 MB]`，旧"尾部翻倍探测"
//! 1/2/4/8/16 MB 都落在 mdat 中间，把 mdat 内随机字节当 box header 解析 → 找不到 moov。
//! 新策略：head 1 MB 找不到 moov 时，跟踪 box 链推算"下一个顶层 box 起点"，从那里 fetch。

use std::time::Duration;

use anyhow::{bail, Result};
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use tracing::{debug, info, warn};

use crate::error::SjtuCliError;

/// 头部探测大小（1 MB）。SJTU CDN moov 实测最大 ~700 KB；faststart 布局通常 head 即可命中。
pub(super) const HEAD_PROBE_SIZE: u64 = 1024 * 1024;
/// mdat 后段的最大 fetch 长度（64 MB）；超过视为异常 mp4（moov 不应该这么大）。
const REMAINDER_MAX: u64 = 64 * 1024 * 1024;
/// chunk 间无字节流入超时（V5.B 第 9 讲事故缓解）。
pub(super) const INTER_BYTE_TIMEOUT: Duration = Duration::from_secs(30);

/// 探测 mp4 size 并定位 moov box，返回 (moov 字节, 已下载字节数)。
/// 策略：1) head 1 MB 找 moov（faststart）；2) 失败则跟踪 box 链推算下一个 box 起点
/// （处理 mdat 占 99% 的 standard 布局）；3) 从 next_box_offset fetch 剩余字节（限 64 MB）找 moov。
pub(super) async fn locate_moov(client: &Client, url: &str) -> Result<(Vec<u8>, u64)> {
    let total = probe_size(client, url).await?;
    if total == 0 {
        bail!("probe size=0：{url}");
    }
    let mut dl: u64 = 1; // probe 1 字节
    let head_end = (HEAD_PROBE_SIZE - 1).min(total - 1);
    let head = fetch_range(client, url, 0, head_end).await?;
    dl += head.len() as u64;
    let head_boxes = dump_top_boxes(&head, 16);
    info!(total, head_len = head.len(), boxes = %head_boxes, "head probe 顶层 box 列表");

    // 路径 1: head 里直接命中 moov（faststart）
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

    // 路径 2: head 推算下一个 box 起点 → 从那里 fetch 后续 box（standard 布局，mdat 占大头）
    let Some(next_box) = next_box_after_head(&head) else {
        bail!("head 1 MB 内无法推算 box 链（mp4 异常）");
    };
    if next_box >= total {
        bail!("推算下一 box 起点 {next_box} 超过文件总长 {total}（mp4 异常）");
    }
    let remainder_len = (total - next_box).min(REMAINDER_MAX);
    let remainder_end = next_box + remainder_len - 1;
    info!(next_box, remainder_len, "从 mdat 后段 fetch 找 moov");
    let remainder = fetch_range(client, url, next_box, remainder_end).await?;
    dl += remainder.len() as u64;
    let remainder_boxes = dump_top_boxes(&remainder, 16);
    info!(boxes = %remainder_boxes, "remainder 顶层 box 列表");
    if let Some((rel, moov_size)) = scan_for_moov(&remainder) {
        if rel as u64 + moov_size <= remainder.len() as u64 {
            return Ok((remainder[rel..rel + moov_size as usize].to_vec(), dl));
        }
        bail!(
            "remainder 内 moov 跨 fetch 边界（moov size={moov_size} > {} MB）",
            REMAINDER_MAX / 1024 / 1024
        );
    }
    warn!(
        head_boxes = %head_boxes,
        remainder_boxes = %remainder_boxes,
        head_hex_64 = %hex_prefix(&head, 64),
        "locate_moov 失败：head 与 mdat 后段都无 moov"
    );
    bail!("head + mdat 后段（{remainder_len} bytes）都找不到 moov，疑似非标准 mp4")
}

/// 在 head buffer 内跟踪完整顶层 box，返回**第一个**超出 head 边界的 box 的绝对起点 offset。
/// 如果所有 box 都在 head 内 / 推算溢出 / size=0 EOF box，返回 None。
fn next_box_after_head(head: &[u8]) -> Option<u64> {
    let head_len = head.len() as u64;
    let mut pos: u64 = 0;
    loop {
        if pos + 8 > head_len {
            // 本 box 头部跨边界 → 它的起点 pos 就是答案
            return Some(pos);
        }
        let pu = pos as usize;
        let s32 = u32::from_be_bytes(head[pu..pu + 4].try_into().ok()?);
        let size: u64 = if s32 == 1 {
            if pos + 16 > head_len {
                return Some(pos);
            }
            u64::from_be_bytes(head[pu + 8..pu + 16].try_into().ok()?)
        } else if s32 == 0 {
            return None;
        } else {
            s32 as u64
        };
        if size < 8 {
            return None;
        }
        pos = pos.checked_add(size)?;
        if pos >= head_len {
            return Some(pos);
        }
    }
}

/// 列出 buf 中前 N 个顶层 box 的 (type, size)，用于诊断 mp4 实际布局。
/// 不依赖 mp4_box 内部解析（保持自包含），尽力扫描，遇到无效 box 立即停止。
fn dump_top_boxes(buf: &[u8], max_n: usize) -> String {
    let mut out = String::new();
    let mut pos = 0usize;
    let mut n = 0;
    while pos + 8 <= buf.len() && n < max_n {
        let s32 = u32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
        let ty = &buf[pos + 4..pos + 8];
        let ty_str = std::str::from_utf8(ty).unwrap_or("????");
        let (size, hdr_len) = if s32 == 1 {
            if pos + 16 > buf.len() {
                break;
            }
            let arr: [u8; 8] = buf[pos + 8..pos + 16].try_into().unwrap();
            (u64::from_be_bytes(arr), 16u64)
        } else if s32 == 0 {
            (buf.len() as u64 - pos as u64, 8u64)
        } else {
            (s32 as u64, 8u64)
        };
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!("[{ty_str} {size}]"));
        if size < hdr_len {
            out.push_str("<bad>");
            break;
        }
        match pos.checked_add(size as usize) {
            Some(np) => pos = np,
            None => break,
        }
        n += 1;
    }
    out
}

fn hex_prefix(buf: &[u8], n: usize) -> String {
    let take = n.min(buf.len());
    let mut s = String::with_capacity(take * 2);
    for b in &buf[..take] {
        let _ = std::fmt::Write::write_fmt(&mut s, format_args!("{b:02x}"));
    }
    s
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

#[cfg(test)]
pub(super) fn next_box_after_head_for_test(head: &[u8]) -> Option<u64> {
    next_box_after_head(head)
}
