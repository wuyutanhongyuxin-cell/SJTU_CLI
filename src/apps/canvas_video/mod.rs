//! 课堂视频（v.sjtu.edu.cn / 交我学）客户端。
//!
//! 与现有 `apps::canvas`（PAT 鉴权的 oc.sjtu 作业 DDL）独立，因为：
//! - 鉴权链是 LTI 1.3 OIDC（不能用 Canvas PAT）
//! - 域名不同（v.sjtu vs oc.sjtu），cookie / endpoint 全不一样
//! - sub-session 文件名 `canvas_video` ≠ `canvas`
//!
//! CP 进度（详见 tasks/canvas_video_investigation.md §10）：
//! - **CP-V1（本模块当前范围）**：LTI launch + token 提取 + list_lectures
//! - CP-V2..V4：list 字段脱敏 / 单讲下载 / 批量 + ffmpeg（未做）

mod api;
mod api_form;
pub mod auth;
mod auth_chrome;
mod cache;
pub mod download;
pub mod ffmpeg;
mod http;
mod models;
mod models_video;
#[cfg(test)]
mod tests_parse;
#[cfg(test)]
mod tests_cache;
mod throttle;

pub use api::{Client, VideoFetch};
pub use models::{Bootstrap, LectureVideo};
