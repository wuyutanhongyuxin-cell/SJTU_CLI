//! `sjtu jwc <sub>` 子命令实现。
//!
//! 模块组织（仿 jwbmessage 的拆分；MVP 阶段尚不需 read/write 二分）：
//! - `handlers.rs`：cmd_grades / cmd_schedule / cmd_gpa / cmd_exams
//! - `data.rs`：Envelope 里承载的 Data struct
//!
//! 端点契约见 tasks/isjtu_investigation.md。

mod data;
mod handlers;

pub use handlers::{cmd_exams, cmd_gpa, cmd_grades, cmd_schedule};
