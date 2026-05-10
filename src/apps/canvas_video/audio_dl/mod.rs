//! audio-only Range 直拉：mp4 box 解析 → 仅取 audio sample 字节 → mux 成 m4a。
//!
//! 与 download.rs 双路并存：
//! - download.rs 走旧 mp4 全下（保留给 keep-mp4 / 视频路径）
//! - audio_dl 走新 audio-only 路径，专属 client（90 s 段级 + 30 s inter-byte）
//!
//! `download_audio_only_to_file` 是对外唯一入口；fail-soft 由 download_shared 层负责
//! （V5.D 上线初期解析失败回退到旧路径）。

mod client;
mod orchestrator;
#[cfg(test)]
mod tests;

pub use orchestrator::{download_audio_only_to_file, DownloadStats};
