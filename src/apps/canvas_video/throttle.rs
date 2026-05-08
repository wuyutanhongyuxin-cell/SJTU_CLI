//! 课堂视频 API 的进程内节流：固定 200ms 间隔。
//!
//! 与 jwc / jwbmessage / canvas 同构。MVP 一次拉一门课的 16 讲，单进程；
//! 200ms 既不触上游限速，又比 300ms 略激进（v.sjtu 后端响应实测 0.5-2s，
//! 节流相对网络耗时可忽略）。

use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tokio::time::sleep;

/// 最小请求间隔：200ms。
pub(super) const MIN_INTERVAL: Duration = Duration::from_millis(200);

/// 进程内共享节流器。每次 HTTP 前调一次 `wait()`。
#[derive(Debug)]
pub(super) struct Throttle {
    last: Mutex<Instant>,
}

impl Throttle {
    /// 构造：把 `last` 置于 `MIN_INTERVAL` 之前，首次调用不 sleep。
    pub fn new() -> Self {
        let seed = Instant::now()
            .checked_sub(MIN_INTERVAL)
            .unwrap_or_else(Instant::now);
        Self {
            last: Mutex::new(seed),
        }
    }

    /// 距上次不足 `MIN_INTERVAL` 则 sleep 补齐，随后刷新记点。
    pub async fn wait(&self) {
        let mut last = self.last.lock().await;
        let elapsed = last.elapsed();
        if elapsed < MIN_INTERVAL {
            sleep(MIN_INTERVAL - elapsed).await;
        }
        *last = Instant::now();
    }
}
