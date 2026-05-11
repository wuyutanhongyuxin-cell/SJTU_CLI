//! audio_dl fetch helpers：并发 Range 拉取 + 重试 + sample 重组。

use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};
use reqwest::Client;
use tokio::sync::Semaphore;
use tracing::warn;

use crate::error::SjtuCliError;

use super::orchestrator::fetch_range;

/// fetch_range 梯度重试，backoff [0, 3s, 10s, 25s]（与 download.rs V3.1 对齐）。
async fn fetch_range_with_retry(
    client: &Client,
    url: &str,
    start: u64,
    end: u64,
) -> Result<Vec<u8>> {
    const BACKOFF_MS: [u64; 4] = [0, 3000, 10000, 25000];
    let mut last: Option<anyhow::Error> = None;
    for (attempt, wait) in BACKOFF_MS.iter().enumerate() {
        tokio::time::sleep(Duration::from_millis(*wait)).await;
        match fetch_range(client, url, start, end).await {
            Ok(b) => return Ok(b),
            Err(e) => {
                warn!(start, end, attempt, err = %e, "段失败重试");
                last = Some(e);
            }
        }
    }
    Err(last.expect("≥1 次尝试"))
}

/// 在 client 池里按 range_idx 哈希取一个 client。
///
/// 策略：简单 modulo（range_idx % pool.len()）。
/// 1201 range × 4 client 池 → 每 client ≈ 300 range，足够利用 H2 multiplex。
/// 规避 reqwest #1276 单 client H2 高并发 buffer 阻塞 bug。
pub(super) fn pick_client(clients: &[Client], range_idx: usize) -> &Client {
    &clients[range_idx % clients.len()]
}

/// V5.E-B+ 主入口：并发拉多个 Range，按 pick_client 分桶到 client 池。
///
/// 返回 Vec<(range_idx, 字节)>，顺序由 JoinSet 完成顺序决定（不保证）。
pub(super) async fn parallel_ranges_pool(
    clients: &[Client],
    url: &str,
    ranges: &[(u64, u64)],
    concurrency: usize,
) -> Result<Vec<(usize, Vec<u8>)>> {
    let sem = Arc::new(Semaphore::new(concurrency));
    let mut joins = tokio::task::JoinSet::new();
    for (i, &(s, e)) in ranges.iter().enumerate() {
        let sem = sem.clone();
        let cli = pick_client(clients, i).clone();
        let url = url.to_string();
        joins.spawn(async move {
            let _permit = sem.acquire_owned().await.expect("semaphore closed");
            let bytes = fetch_range_with_retry(&cli, &url, s, e).await?;
            Ok::<_, anyhow::Error>((i, bytes))
        });
    }
    let mut out: Vec<(usize, Vec<u8>)> = Vec::with_capacity(ranges.len());
    while let Some(r) = joins.join_next().await {
        out.push(r.map_err(|e| SjtuCliError::NetworkError(format!("task panic: {e}")))??);
    }
    Ok(out)
}

/// 把若干 Range 字节按 sample 顺序拼回去。
/// fetched: Vec<(range_idx, range_bytes)>，顺序任意。
pub(super) fn reassemble_samples(
    track: &crate::apps::canvas_video::mp4_box::AudioTrack,
    ranges: &[(u64, u64)],
    fetched: &[(usize, Vec<u8>)],
) -> Result<Vec<u8>> {
    let mut by_idx: Vec<Option<&Vec<u8>>> = vec![None; ranges.len()];
    for (i, b) in fetched {
        by_idx[*i] = Some(b);
    }
    let mut out: Vec<u8> = Vec::with_capacity(track.sample_sizes.iter().map(|&s| s as usize).sum());
    for (&off, &sz) in track.sample_offsets.iter().zip(track.sample_sizes.iter()) {
        let (ri, range_start) = find_range_for_sample(ranges, off)
            .ok_or_else(|| anyhow::anyhow!("sample offset {off} 不在任何 Range"))?;
        let bytes = by_idx[ri].ok_or_else(|| anyhow::anyhow!("Range {ri} 没有数据（拉取丢失）"))?;
        let local_start = (off - range_start) as usize;
        let local_end = local_start + sz as usize;
        if local_end > bytes.len() {
            bail!(
                "sample 越界：range {ri} len={} 需 {}..{}",
                bytes.len(),
                local_start,
                local_end
            );
        }
        out.extend_from_slice(&bytes[local_start..local_end]);
    }
    Ok(out)
}

/// 在 ranges 里二分找 sample offset 落入哪个 range，返 (idx, range_start)。
/// ranges 必须按起点升序（merge_ranges 产出天然满足）。
fn find_range_for_sample(ranges: &[(u64, u64)], offset: u64) -> Option<(usize, u64)> {
    let mut lo = 0usize;
    let mut hi = ranges.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        let (s, e) = ranges[mid];
        if offset < s {
            hi = mid;
        } else if offset > e {
            lo = mid + 1;
        } else {
            return Some((mid, s));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;

    #[test]
    fn pick_client_distributes_by_modulo() {
        let pool: Vec<Client> = (0..4).map(|_| Client::new()).collect();
        // 同一 idx 两次取同一 client（指针相等）
        for i in 0..20 {
            let c1 = pick_client(&pool, i);
            let c2 = pick_client(&pool, i);
            assert!(std::ptr::eq(c1, c2));
        }
        // idx N 应回到 client idx % pool.len()
        assert!(std::ptr::eq(pick_client(&pool, 0), pick_client(&pool, 4)));
        assert!(std::ptr::eq(pick_client(&pool, 1), pick_client(&pool, 5)));
        assert!(std::ptr::eq(pick_client(&pool, 7), pick_client(&pool, 11)));
    }
}
