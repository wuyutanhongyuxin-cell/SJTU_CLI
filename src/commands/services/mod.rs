//! `sjtu services <sub>` 子命令实现。
//!
//! MVP 命令清单：
//! - `pending [--with-identity]` —— 待办列表（按 `code=="ADD"` 拆"我申请的" / "等我处理的"）
//!
//! 端点契约见 tasks/isjtu_investigation.md §6.3 N-todo。

mod data;
mod handlers;

pub use handlers::cmd_pending;
