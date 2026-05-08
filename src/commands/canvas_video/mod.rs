//! `sjtu canvas-video <sub>` 子命令实现。
//!
//! 模块组织：
//! - `handlers.rs`：list（CP-V1 唯一命令）
//! - `data.rs`：Envelope 里承载的 Data struct（PII 抹掉的视图）
//!
//! 端点契约见 tasks/canvas_video_investigation.md。

mod data;
mod download_handler;
mod handlers;

pub use download_handler::cmd_download;
pub use handlers::cmd_list;
