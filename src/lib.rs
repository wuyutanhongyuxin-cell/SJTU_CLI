//! SJTU-CLI —— 上海交通大学"交我办"终端命令行工具。
//!
//! S0 骨架 + S1 扫码登录：CLI 入口 + 基础设施 + auth/commands 子模块。

pub mod apps;
pub mod auth;
pub mod cli;
pub mod commands;
pub mod config;
pub mod cookies;
pub mod error;
pub mod output;
pub mod util;

/// 版本号，来自 Cargo.toml 的 `package.version`。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
