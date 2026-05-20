//! 固定 sleep 节流：每次 weijieyue 端点调用前强制间隔 ≥ MIN_INTERVAL。
//!
//! 与 elec / services / jwbmessage 同范式，独立一份避免跨子系统耦合
//! （CLAUDE.md 项目专属约束）。weijieyue 后端未观测限速，300ms 既守稳又几乎不感知。

use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tokio::time::sleep;

pub(super) const MIN_INTERVAL: Duration = Duration::from_millis(300);

#[derive(Debug)]
pub(super) struct Throttle {
    last: Mutex<Instant>,
}

impl Throttle {
    pub fn new() -> Self {
        let seed = Instant::now()
            .checked_sub(MIN_INTERVAL)
            .unwrap_or_else(Instant::now);
        Self {
            last: Mutex::new(seed),
        }
    }

    pub async fn wait(&self) {
        let mut last = self.last.lock().await;
        let elapsed = last.elapsed();
        if elapsed < MIN_INTERVAL {
            sleep(MIN_INTERVAL - elapsed).await;
        }
        *last = Instant::now();
    }
}
