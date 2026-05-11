//! audio_dl 专属 reqwest Client 池：4 个独立 Client × H2 ALPN multiplex。
//!
//! V5.E-B+ 升级（见 docs/superpowers/specs/2026-05-11-v5e-b-plus-multi-h2-p85-design.md）：
//! V5.D 单 Client + http1_only + pool_max_idle_per_host(0) 关 H2 强制 H1.1 × 8 TCP。
//! V5.E-B+ 改为 4 独立 Client，让 reqwest ALPN 协商 H2；1201 range 由 fetch.rs 哈希分桶
//! 到 4 个池成员，每连 100 H2 stream → 总 400 stream，规避 reqwest #1276 单 client
//! H2 高并发 buffer 阻塞 bug。
//!
//! ## Env vars
//! - `SJTU_FORCE_HTTP1=1` → 退回 V5.D 单 Client + H1.1 行为（H2 真机不利时兜底）
//! - `SJTU_H2_POOL_SIZE=<1..=16>` → override 池大小，默认 4，invalid 取默认

use std::time::Duration;

use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, REFERER, USER_AGENT};
use reqwest::Client;

use crate::apps::canvas_video::http::UA;
use crate::error::SjtuCliError;

/// 默认池大小（V5.E-B+ 实测 4 平衡 reqwest #1276 防御 vs CDN 限流风险）。
const DEFAULT_POOL_SIZE: usize = 4;
/// 池大小允许范围（防 env 配置爆炸）。
const POOL_SIZE_MIN: u8 = 1;
const POOL_SIZE_MAX: u8 = 16;

/// 构造 audio-only 下载 client 池。
///
/// 返回 N 个独立 reqwest::Client（每个独立 H2 连接）。调用方按 range_idx 哈希分桶。
///
/// # Env
/// - SJTU_FORCE_HTTP1=1 → 返单 Client（H1.1 + pool_max_idle_per_host(0)），V5.D 行为
/// - SJTU_H2_POOL_SIZE=N（1-16, default 4, invalid → default）
pub(super) fn build_client_pool_audio(referer: &str) -> Result<Vec<Client>> {
    // HeaderValue::from_str 只拒绝部分控制字符；显式验证 ASCII-only 以拒绝 IDN / 非 ASCII 域名。
    if !referer.is_ascii() {
        return Err(
            SjtuCliError::InvalidInput(format!("Referer 含非 ASCII 字符：{referer}")).into(),
        );
    }
    let mut h = HeaderMap::new();
    h.insert(
        REFERER,
        HeaderValue::from_str(referer)
            .map_err(|e| SjtuCliError::InvalidInput(format!("Referer 无效: {e}")))?,
    );
    h.insert(USER_AGENT, HeaderValue::from_static(UA));

    let force_h1 = std::env::var("SJTU_FORCE_HTTP1").as_deref() == Ok("1");
    let pool_size: usize = if force_h1 {
        1
    } else {
        std::env::var("SJTU_H2_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse::<u8>().ok())
            .filter(|&n| (POOL_SIZE_MIN..=POOL_SIZE_MAX).contains(&n))
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_POOL_SIZE)
    };

    let mut pool = Vec::with_capacity(pool_size);
    for _ in 0..pool_size {
        let mut b = Client::builder()
            .default_headers(h.clone())
            .tcp_nodelay(true)
            .tcp_keepalive(Duration::from_secs(60))
            .timeout(Duration::from_secs(90))
            .connect_timeout(Duration::from_secs(15));
        if force_h1 {
            b = b.http1_only().pool_max_idle_per_host(0);
        }
        pool.push(
            b.build()
                .map_err(|e| SjtuCliError::NetworkError(format!("audio_dl client: {e}")))?,
        );
    }
    Ok(pool)
}

/// V5.D 兼容入口 — T3/T4 完成后将删除。
///
/// thin wrapper：单纯取 pool[0]，行为与 V5.D 相同（H1.1 or H2，视 SJTU_FORCE_HTTP1）。
/// 保留是为了让 orchestrator.rs / tests.rs 在 T1 commit 后仍能编译；
/// T3/T4 完成时直接删本函数。
pub(super) fn build_client_audio(referer: &str) -> Result<Client> {
    let pool = build_client_pool_audio(referer)?;
    Ok(pool.into_iter().next().expect("pool 至少 1"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// env var 竞争保护锁：cargo test 默认多线程，三个 env 测必须串行执行。
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn pool_default_size_is_4() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("SJTU_FORCE_HTTP1");
        std::env::remove_var("SJTU_H2_POOL_SIZE");
        let pool = build_client_pool_audio("https://courses.sjtu.edu.cn").unwrap();
        assert_eq!(pool.len(), 4);
    }

    #[test]
    fn pool_respects_size_env() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("SJTU_FORCE_HTTP1");
        std::env::set_var("SJTU_H2_POOL_SIZE", "8");
        let pool = build_client_pool_audio("https://courses.sjtu.edu.cn").unwrap();
        assert_eq!(pool.len(), 8);
        std::env::remove_var("SJTU_H2_POOL_SIZE");
    }

    #[test]
    fn force_http1_falls_back_to_single() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("SJTU_H2_POOL_SIZE");
        std::env::set_var("SJTU_FORCE_HTTP1", "1");
        let pool = build_client_pool_audio("https://courses.sjtu.edu.cn").unwrap();
        assert_eq!(pool.len(), 1);
        std::env::remove_var("SJTU_FORCE_HTTP1");
    }
}
