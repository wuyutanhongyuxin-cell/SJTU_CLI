//! 固定 sleep 节流：每次 i.sjtu API 调用前强制间隔 ≥ MIN_INTERVAL。
//!
//! 与 shuiyuan / jwbmessage 同策略；ZF 系统压力下仍稳，500ms 比 jwbmessage 的 300ms
//! 略高，因为 ZF 的 GET-via-POST 是真查询（数据库扫描），后端比纯消息推送贵。

use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tokio::time::sleep;

pub(super) const MIN_INTERVAL: Duration = Duration::from_millis(500);

/// 进程内共享节流器。`wait()` 前调，刷新记点。
#[derive(Debug)]
pub(super) struct Throttle {
    last: Mutex<Instant>,
}

impl Throttle {
    /// 构造：把 `last` 置 MIN_INTERVAL 前，首次调用不 sleep。
    pub fn new() -> Self {
        let seed = Instant::now()
            .checked_sub(MIN_INTERVAL)
            .unwrap_or_else(Instant::now);
        Self {
            last: Mutex::new(seed),
        }
    }

    /// 距上次不足 MIN_INTERVAL 则 sleep 补齐，随后刷新记点。
    pub async fn wait(&self) {
        let mut last = self.last.lock().await;
        let elapsed = last.elapsed();
        if elapsed < MIN_INTERVAL {
            sleep(MIN_INTERVAL - elapsed).await;
        }
        *last = Instant::now();
    }
}
