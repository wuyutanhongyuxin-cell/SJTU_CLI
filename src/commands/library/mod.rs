//! `sjtu library <sub>` 子命令实现。
//!
//! MVP 三件套（均**只读**）：
//! - `loans` —— 当前借阅
//! - `history` —— 历史借阅
//! - `fines` —— 罚款（仅显示，不点缴费）
//!
//! 端点契约：docs/superpowers/plans/2026-05-20-t7-library-loans.md。

mod data;
mod handlers;

pub use handlers::{cmd_fines, cmd_history, cmd_loans};
