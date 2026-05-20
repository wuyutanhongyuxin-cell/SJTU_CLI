//! 固定 sleep 节流：mail SOAP 调用前强制间隔 ≥ MIN_INTERVAL。
//!
//! 与 library / elec / services / jwbmessage 同范式，独立一份避免跨子系统耦合。
//! Zimbra 后端是大学多人共用集群，连发 SOAP 容易触发限流；300ms 固定间隔保守安全。

use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const MIN_INTERVAL: Duration = Duration::from_millis(300);

pub(super) struct Throttle {
    last: Mutex<Option<Instant>>,
}

impl Throttle {
    pub(super) fn new() -> Self {
        Self {
            last: Mutex::new(None),
        }
    }

    /// 阻塞直到距上次调用 ≥ MIN_INTERVAL。
    pub(super) async fn wait(&self) {
        let mut guard = self.last.lock().await;
        if let Some(t) = *guard {
            let elapsed = t.elapsed();
            if elapsed < MIN_INTERVAL {
                tokio::time::sleep(MIN_INTERVAL - elapsed).await;
            }
        }
        *guard = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn throttle_first_call_is_immediate() {
        let t = Throttle::new();
        let start = Instant::now();
        t.wait().await;
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn throttle_second_call_waits() {
        let t = Throttle::new();
        t.wait().await;
        let start = Instant::now();
        t.wait().await;
        assert!(start.elapsed() >= MIN_INTERVAL);
    }
}
