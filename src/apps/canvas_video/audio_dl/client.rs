//! audio_dl 专属 reqwest Client：90 s 段级 timeout，关 H2 + 关池（继承 V3.1 经验）。

use std::time::Duration;

use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, REFERER, USER_AGENT};
use reqwest::Client;

use crate::apps::canvas_video::http::UA;
use crate::error::SjtuCliError;

/// audio-only 下载入口的 reqwest Client。
///
/// 与 download.rs 的旧 client 区别：timeout 从 30 min 收紧到 90 s（audio sample 单段
/// 通常 < 5 MB，CDN 限速 1 MB/s 也只要 5 s；90 s 给 18× 安全垫，单段卡死自动 abort）。
/// 90 s 段级 timeout 与 chunk 间 30 s inter-byte timeout（在 orchestrator 内联）共同
/// 守护 V5.B Phase 1 第 9 讲那种 "TCP 不断但 body 无字节流入 13 min" 场景。
#[allow(dead_code)] // T6+ orchestrator 实装后移除
pub(super) fn build_client_audio(referer: &str) -> Result<Client> {
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
    Client::builder()
        .default_headers(h)
        .http1_only()
        .pool_max_idle_per_host(0)
        .tcp_nodelay(true)
        .timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(15))
        // 保留系统代理：SJTU 用户可能需要通过企业/政府代理访问 courses.sjtu.edu.cn
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("audio_dl client: {e}")).into())
}
