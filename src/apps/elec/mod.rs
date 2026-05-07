//! 生活服务 — 宿舍电费（`elec.sjtu.edu.cn`）客户端 —— S3e MVP。
//!
//! 职责：
//! - CAS 跳转到 `https://elec.sjtu.edu.cn/`，注入 JSESSIONID / keepalive cookie
//!   到独立 sub-session 文件 `elec.json`（与 my.sjtu / jwc 不共享，§7.1）
//! - 封装四个只读端点：
//!     - `GET /api/me/info` —— 当前用户绑定房间
//!     - `GET /api/ws/sydl` —— 余额（**关键**，字符串金额）
//!     - `GET /api/rechage/ydl?year=&month=` —— 月度聚合（数字金额）
//!     - `GET /api/rechage/ydlmx?begin=&end=` —— 日明细（数字金额）
//!
//! **金额硬约束**（CLAUDE.md 项目专属）：所有金额 / 度数字段一律
//! `rust_decimal::Decimal`，绝不用 f32/f64；混合 string/number 类型由
//! `models::decimal_str_or_num` 自定义 ser/de 统一兜底。
//!
//! 一卡通余额延 phase-2：web SP 路径不存在（`taskcenter://` 移动 deep-link）+
//! `ecard.sjtu.edu.cn` 校园网限制。
//!
//! 路径契约：tasks/isjtu_investigation.md §7。

mod api;
mod http;
mod models;
#[cfg(test)]
mod tests_parse;
mod throttle;

pub use api::{Client, LoginMeta};
pub use models::{Balance, DailyUsage, MeInfo, Monthly};
