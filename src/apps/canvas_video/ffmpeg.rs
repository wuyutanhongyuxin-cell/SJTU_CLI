//! ffmpeg 子进程封装：把 mp4 抽成 m4a（`-vn -acodec copy`），不重编码、不带原视频流。
//!
//! 设计要点：
//! - 启动期不预检 ffmpeg（CLI 启动慢），只在 `--audio-only` 路径触发 `ensure_ffmpeg()`
//! - 不在场给清晰指引：winget / brew / apt 三平台一句话
//! - 子进程 stderr 截短上行（避免 mp4 路径全文 / ffmpeg 大段 banner 落日志）
//! - 同步 std::process 即可（mp4 已落盘，抽流是串行后置步骤，不需要 async）
//! - `-y` 覆盖已有 m4a；`-loglevel error` 抑制 ffmpeg 默认 banner

use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::SjtuCliError;
use anyhow::Result;

/// 检测 ffmpeg 是否可用：跑 `ffmpeg -version` 看 status。
/// 缺则给装 ffmpeg 的指引（winget / brew / apt 三平台）。
pub fn ensure_ffmpeg() -> Result<()> {
    let probe = Command::new("ffmpeg")
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match probe {
        Ok(s) if s.success() => Ok(()),
        _ => Err(SjtuCliError::InvalidInput(
            "未检测到 ffmpeg。--audio-only 需本地装 ffmpeg：\n  \
             Windows: winget install --id Gyan.FFmpeg\n  \
             macOS:   brew install ffmpeg\n  \
             Linux:   sudo apt install ffmpeg / sudo dnf install ffmpeg"
                .into(),
        )
        .into()),
    }
}

/// `ffmpeg -loglevel error -y -i <mp4> -vn -acodec copy <m4a>`。失败时附 stderr 短摘。
/// 阻塞调用：mp4 已落盘，调用方 `tokio::task::spawn_blocking` 包一层即可不阻塞 runtime。
pub fn extract_audio_blocking(mp4: &Path, m4a: &Path) -> Result<()> {
    let output = Command::new("ffmpeg")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(mp4)
        .arg("-vn")
        .arg("-acodec")
        .arg("copy")
        .arg(m4a)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| SjtuCliError::NetworkError(format!("启动 ffmpeg 失败：{e}")))?;
    if !output.status.success() {
        let snippet = String::from_utf8_lossy(&output.stderr);
        let head: String = snippet.lines().take(3).collect::<Vec<_>>().join(" | ");
        return Err(SjtuCliError::UpstreamError(format!(
            "ffmpeg 抽流失败 (status={:?})：{}",
            output.status.code(),
            head
        ))
        .into());
    }
    Ok(())
}

/// async 包装：tokio `spawn_blocking` 跑 ffmpeg，避免阻塞 runtime。
pub async fn extract_audio(mp4: &Path, m4a: &Path) -> Result<()> {
    let mp4_owned = mp4.to_path_buf();
    let m4a_owned = m4a.to_path_buf();
    tokio::task::spawn_blocking(move || extract_audio_blocking(&mp4_owned, &m4a_owned))
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("ffmpeg 任务 join 失败：{e}")))?
}
