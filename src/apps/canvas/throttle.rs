//! Canvas API 节流器：300ms 间隔。
//!
//! Canvas 官方限速是 600 单位桶（`x-rate-limit-remaining` header），
//! 按请求成本扣（planner/items 约 0.17/次，约 3500 次/min）；我们取 300ms
//! 对齐 shuiyuan / jwbmessage 策略，既不触顶也不感知延迟。

use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tokio::time::sleep;

pub(super) const MIN_INTERVAL: Duration = Duration::from_millis(300);

/// 进程内共享节流器。每次 HTTP 前调一次 `wait()`。
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

    /// 距上次不足 MIN_INTERVAL 则 sleep 补齐。
    pub async fn wait(&self) {
        let mut last = self.last.lock().await;
        let elapsed = last.elapsed();
        if elapsed < MIN_INTERVAL {
            sleep(MIN_INTERVAL - elapsed).await;
        }
        *last = Instant::now();
    }
}
