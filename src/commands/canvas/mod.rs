//! `sjtu canvas <sub>` 子命令实现。
//!
//! 模块组织：
//! - `handlers.rs`：setup / whoami / today / upcoming
//! - `data.rs`：Envelope 里承载的 Data struct
//!
//! 端点契约见 tasks/s3c-canvas-planner.md。

mod data;
mod handlers;

pub use handlers::{cmd_setup, cmd_today, cmd_upcoming, cmd_whoami};
