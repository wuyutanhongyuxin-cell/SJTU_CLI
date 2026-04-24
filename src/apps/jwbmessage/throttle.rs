//! 固定 sleep 节流：每次 `/api/jwbmessage/*` 调用前强制间隔 ≥ MIN_INTERVAL。
//!
//! 与 shuiyuan 的 throttle 相同策略；这里独立一份，避免跨子系统耦合。
//! 交我办后端没见明显限速，但 300ms 既守稳又几乎不感知。

use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tokio::time::sleep;

pub(super) const MIN_INTERVAL: Duration = Duration::from_millis(300);

/// 进程内共享节流器。`wait()` 前调、`Instant` 记点。
#[derive(Debug)]
pub(super) struct Throttle {
    last: Mutex<Instant>,
}

impl Throttle {
    /// 构造：把 `last` 置于 300ms 前，首次调用不 sleep。
    pub fn new() -> Self {
        let seed = Instant::now()
            .checked_sub(MIN_INTERVAL)
            .unwrap_or_else(Instant::now);
        Self {
            last: Mutex::new(seed),
        }
    }

    /// 如距上次不足 `MIN_INTERVAL` 则 sleep 补齐，随后刷新记点。
    pub async fn wait(&self) {
        let mut last = self.last.lock().await;
        let elapsed = last.elapsed();
        if elapsed < MIN_INTERVAL {
            sleep(MIN_INTERVAL - elapsed).await;
        }
        *last = Instant::now();
    }
}
