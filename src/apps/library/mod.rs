//! 图书馆借阅子系统（weijieyue.lib.sjtu.edu.cn:8080）—— T7 MVP。
//!
//! 职责：
//! - 主 jaccount session 注入 reqwest jar（与 weixin path 同范式）
//! - `Client::connect` 触发服务端 OAuth dance，兑 JSESSIONID
//! - 三个只读端点：当前借阅 / 历史借阅 / 罚款
//!
//! **红线**（CLAUDE.md）：永不实装 renew / generageDoPayData / updateCash / checkIsPaid 写端点。
//!
//! 路径契约：docs/superpowers/plans/2026-05-20-t7-library-loans.md。

mod client;
mod http;
mod models;
#[cfg(test)]
mod tests_parse;
mod throttle;

pub use client::{Client, LoginMeta};
pub use models::{Fine, HistoryRow, Loan};
