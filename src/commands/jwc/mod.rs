//! `sjtu jwc <sub>` 子命令实现。
//!
//! 模块组织（仿 jwbmessage 的拆分；MVP 阶段尚不需 read/write 二分）：
//! - `handlers.rs`：cmd_grades / cmd_schedule / cmd_gpa / cmd_exams
//! - `gpa_handlers.rs`：cmd_gpa_by_semester（多学期循环聚合）
//! - `data/`：Envelope 里承载的 Data struct（mod.rs + gpa.rs）
//!
//! 端点契约见 tasks/isjtu_investigation.md。

mod data;
mod gpa_handlers;
mod handlers;
mod ical;
mod schedule_handlers;
mod schedule_helpers;
mod schedule_next;

pub use gpa_handlers::cmd_gpa_by_semester;
pub use handlers::{cmd_exams, cmd_gpa, cmd_grades, cmd_schedule};
pub use ical::handler::cmd_calendar;
pub use schedule_handlers::{cmd_today, cmd_week};
pub use schedule_next::cmd_next;
