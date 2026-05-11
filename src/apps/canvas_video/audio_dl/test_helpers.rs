//! audio_dl 测试专用 helpers：no_proxy client 构造 + 测试入口包装。
//!
//! 不影响生产路径；只在 cfg(test) 下编译。

use std::time::Duration;

use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, REFERER, USER_AGENT};
use reqwest::Client;

use crate::apps::canvas_video::http::UA;
use crate::error::SjtuCliError;

/// 构建 no_proxy 测试 client（绕系统代理，mockito 在 127.0.0.1 否则被代理 503）。
fn build_no_proxy_client(referer: &str) -> Result<Client> {
    let mut h = HeaderMap::new();
    h.insert(
        REFERER,
        HeaderValue::from_str(referer)
            .map_err(|e| SjtuCliError::InvalidInput(format!("Referer 非 ASCII: {e}")))?,
    );
    h.insert(USER_AGENT, HeaderValue::from_static(UA));
    Client::builder()
        .default_headers(h)
        .http1_only()
        .pool_max_idle_per_host(0)
        .tcp_nodelay(true)
        .timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(15))
        .no_proxy()
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("test client: {e}")).into())
}

/// 仅供测试使用的 fetch_range 入口。
pub(super) async fn fetch_range_for_test(
    url: &str,
    referer: &str,
    start: u64,
    end: u64,
) -> Result<Vec<u8>> {
    let client = build_no_proxy_client(referer)?;
    super::orchestrator::fetch_range(&client, url, start, end).await
}

/// 仅供测试使用的 moov 定位入口。
pub(super) async fn locate_moov_for_test(url: &str, referer: &str) -> Result<(Vec<u8>, u64)> {
    let client = build_no_proxy_client(referer)?;
    super::orchestrator::locate_moov(&client, url).await
}
