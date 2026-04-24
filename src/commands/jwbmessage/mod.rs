//! `sjtu messages <sub>` 子命令实现。
//!
//! 模块组织（仿 shuiyuan 的拆分）：
//! - `handlers_read.rs`：list / show（list 不触发已读；show **会**触发已读副作用）
//! - `handlers_write.rs`：read-all（强制 `--yes`）
//! - `data.rs`：Envelope 里承载的 Data struct
//!
//! 端点契约见 tasks/s3b-jiaowoban-messages.md。

mod data;
mod handlers_read;
mod handlers_write;

pub use handlers_read::{cmd_list, cmd_show};
pub use handlers_write::cmd_read_all;
