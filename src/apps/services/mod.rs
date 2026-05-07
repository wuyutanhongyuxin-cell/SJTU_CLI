//! 办事大厅（my.sjtu.edu.cn）客户端 —— S3d MVP。
//!
//! 职责：
//! - CAS 跳转到 `/ui/app/` 注入 JSESSIONID / keepalive / PORTAL_LOCALE cookie
//!   （与 `apps/jwbmessage` 共享同一个 my.sjtu 后端，但 sub-session 文件独立 `services.json`）
//! - 封装 `/api/task/me/processes/todo` 只读端点（MVP 仅待办）
//!
//! 路径契约：tasks/isjtu_investigation.md §6（特别 §6.3 N-todo / §6.6 已知坑）。

mod api;
mod http;
mod models;
#[cfg(test)]
mod tests_parse;
mod throttle;

pub use api::{Client, LoginMeta};
pub use models::{App, Milestone, Owner, Process, TodoItem};
