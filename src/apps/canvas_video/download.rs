//! mp4 直链 Range 分片并发下载。`GET Range: bytes=0-0` 探测 size，分片合并到 `<dest>.tmp`
//! 再原子 `rename`。段失败梯度 backoff [0, 3s, 10s, 25s]，mp4 URL `key=` 1-3h 内有效。
//!
//! **CP-V3.1 加速**：reqwest 默认 H2 让 N 段复用同一条 TCP，被 SJTU CDN 按 per-conn 整体
//! 限速 ~1MB/s。`.http1_only()` + `.pool_max_idle_per_host(0)` 强制每段独立 TCP，对照
//! prcwcy/sjtu-canvas-video-download 的 aria2 -x 16 实测路径。

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_RANGE, RANGE, REFERER, USER_AGENT};
use reqwest::{Client, StatusCode};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};

use super::http::UA;
use crate::error::SjtuCliError;

/// 下载 `url` 到 `dest`，最多 `concurrency` 个并发分片（< 2 即走单段）。
/// `referer` 必带 `https://courses.sjtu.edu.cn`（SJTU CDN 实测必需）。返回写入字节数。
pub async fn download_to_file(
    url: &str,
    dest: &Path,
    concurrency: usize,
    referer: &str,
) -> Result<u64> {
    let client = build_client(referer)?;
    let (size, partial) = probe_size(&client, url).await?;
    if size == 0 {
        return Err(SjtuCliError::UpstreamError(format!("probe size=0：{url}")).into());
    }
    let n = if partial { concurrency.max(1) } else { 1 };
    info!(size, concurrency = n, partial, "开始下载 mp4");
    parallel_ranges(&client, url, dest, size, n as u64).await
}

fn build_client(referer: &str) -> Result<Client> {
    let mut h = HeaderMap::new();
    h.insert(
        REFERER,
        HeaderValue::from_str(referer)
            .map_err(|e| SjtuCliError::InvalidInput(format!("Referer 非 ASCII: {e}")))?,
    );
    h.insert(USER_AGENT, HeaderValue::from_static(UA));
    // http1_only + pool_max_idle_per_host(0)：每段一条独立 TCP，绕过 H2 per-conn 限速。
    Client::builder()
        .default_headers(h)
        .http1_only()
        .pool_max_idle_per_host(0)
        .tcp_nodelay(true)
        .timeout(Duration::from_secs(60 * 30))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("download client: {e}")).into())
}

/// `GET Range: bytes=0-0` 探测：206 → 从 `Content-Range: bytes 0-0/<total>` 取 size +
/// 标 partial=true；200 → 整段（不分片），从 `Content-Length` 取 size。
async fn probe_size(client: &Client, url: &str) -> Result<(u64, bool)> {
    let resp = client
        .get(url)
        .header(RANGE, "bytes=0-0")
        .send()
        .await
        .map_err(neterr("probe"))?;
    let st = resp.status();
    if !st.is_success() && st != StatusCode::PARTIAL_CONTENT {
        return Err(SjtuCliError::UpstreamError(format!("probe status={st}")).into());
    }
    let partial = st == StatusCode::PARTIAL_CONTENT;
    let total: u64 = if partial {
        resp.headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit('/').next())
            .and_then(|t| t.parse().ok())
            .unwrap_or(0)
    } else {
        resp.content_length().unwrap_or(0)
    };
    Ok((total, partial && total > 0))
}

async fn parallel_ranges(
    client: &Client,
    url: &str,
    dest: &Path,
    size: u64,
    n: u64,
) -> Result<u64> {
    let chunk = size.div_ceil(n);
    let mut joins = tokio::task::JoinSet::new();
    let mut spawned = 0u64;
    for i in 0..n {
        let start = i * chunk;
        if start >= size {
            break;
        }
        let end = ((i + 1) * chunk - 1).min(size - 1);
        let part = with_ext(dest, &format!("part{i}"));
        let cli = client.clone();
        let url = url.to_string();
        // 段内 sleep i*50ms 错峰：CDN 看到 SYN 间隔均匀 50ms，不阻塞外层 spawn。
        joins.spawn(async move {
            tokio::time::sleep(Duration::from_millis(50 * i)).await;
            range_with_retry(&cli, &url, &part, start, end).await
        });
        spawned += 1;
    }
    let mut total = 0u64;
    while let Some(r) = joins.join_next().await {
        total += r.map_err(|e| SjtuCliError::NetworkError(format!("task panic: {e}")))??;
    }
    merge_parts(dest, spawned).await?;
    Ok(total)
}

async fn merge_parts(dest: &Path, n: u64) -> Result<()> {
    let tmp = with_ext(dest, "tmp");
    let mut out = File::create(&tmp).await.map_err(ioerr("merge create"))?;
    for i in 0..n {
        let part = with_ext(dest, &format!("part{i}"));
        let bytes = tokio::fs::read(&part).await.map_err(ioerr("read part"))?;
        out.write_all(&bytes).await.map_err(ioerr("merge write"))?;
    }
    out.flush().await.ok();
    drop(out);
    tokio::fs::rename(&tmp, dest)
        .await
        .map_err(ioerr("rename"))?;
    for i in 0..n {
        let _ = tokio::fs::remove_file(with_ext(dest, &format!("part{i}"))).await;
    }
    Ok(())
}

async fn range_with_retry(
    client: &Client,
    url: &str,
    path: &Path,
    start: u64,
    end: u64,
) -> Result<u64> {
    // SJTU 教学 CDN 高峰期 504 多发，linear backoff 太短会全军覆没；改梯度 0/3s/10s/25s。
    const BACKOFF_MS: [u64; 4] = [0, 3000, 10000, 25000];
    let mut last: Option<anyhow::Error> = None;
    for (attempt, wait) in BACKOFF_MS.iter().enumerate() {
        tokio::time::sleep(Duration::from_millis(*wait)).await;
        match range_once(client, url, path, start, end).await {
            Ok(n) => {
                debug!(start, end, n, attempt, "段完成");
                return Ok(n);
            }
            Err(e) => {
                warn!(start, end, attempt, err = %e, "段失败重试");
                last = Some(e);
            }
        }
    }
    Err(last.expect("≥1 次尝试"))
}

async fn range_once(client: &Client, url: &str, path: &Path, start: u64, end: u64) -> Result<u64> {
    let rv = format!("bytes={start}-{end}");
    let mut resp = client
        .get(url)
        .header(RANGE, &rv)
        .send()
        .await
        .map_err(neterr("GET range"))?;
    let st = resp.status();
    if st != StatusCode::PARTIAL_CONTENT && !st.is_success() {
        return Err(SjtuCliError::UpstreamError(format!("段 {rv} status={st}")).into());
    }
    let mut file = File::create(path).await.map_err(ioerr("create part"))?;
    let mut written = 0u64;
    while let Some(c) = resp.chunk().await.map_err(neterr("chunk"))? {
        file.write_all(&c).await.map_err(ioerr("write"))?;
        written += c.len() as u64;
    }
    file.flush().await.ok();
    Ok(written)
}

fn with_ext(p: &Path, ext: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".");
    s.push(ext);
    PathBuf::from(s)
}

fn neterr(ctx: &'static str) -> impl Fn(reqwest::Error) -> SjtuCliError {
    move |e| SjtuCliError::NetworkError(format!("{ctx}: {e}"))
}

fn ioerr(ctx: &'static str) -> impl Fn(std::io::Error) -> SjtuCliError {
    move |e| SjtuCliError::NetworkError(format!("{ctx}: {e}"))
}
