//! `sjtu canvas-video <sub>` 子命令实现。
//!
//! 模块组织：
//! - `handlers.rs`：list（CP-V1）+ resolve_target / list_audited_sorted / 脱敏小工具
//! - `download_handler.rs`：download 单讲 + --all-channels（CP-V3 / CP-V3.2 / CP-V4 audio-only）
//! - `batch_handler.rs`：download --lectures SPEC 批量（CP-V4，fail-soft + 临用临取 + 断点续传）
//! - `lectures_spec.rs`：`--lectures` 选择器解析
//! - `data.rs`：Envelope 里承载的 Data struct（PII 抹掉的视图）
//!
//! 端点契约见 tasks/canvas_video_investigation.md。

mod batch_handler;
mod data;
mod download_handler;
mod handlers;
mod lectures_spec;
mod retry;

pub use batch_handler::{cmd_download_batch, BatchArgs};
pub use download_handler::{cmd_download, cmd_download_all};
pub use handlers::{cmd_clear_cache, cmd_list};
