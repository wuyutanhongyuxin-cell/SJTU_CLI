//! Canvas LMS（oc.sjtu.edu.cn）客户端。
//!
//! 职责：
//! - 从 `<config_dir>/sub_sessions/canvas_token.txt` 读 PAT 注入 `Authorization: Bearer`
//! - 封装 `/api/v1/users/self`（whoami）+ `/api/v1/planner/items`（DDL 聚合）
//! - MVP 全只读；不做 courses / inbox / announcements / 写操作
//!
//! 路径契约：tasks/s3c-canvas-planner.md。

mod api;
pub mod auth;
mod http;
mod models;
#[cfg(test)]
mod tests_parse;
mod throttle;

pub use api::Client;
pub use models::{Plannable, PlannerItem, Profile, Submissions};
