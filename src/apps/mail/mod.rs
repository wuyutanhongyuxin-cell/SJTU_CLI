//! `sjtu mail` 子系统：Zimbra SOAP 只读访问。
//!
//! 公开 API：
//! - [`MailClient`] —— SSO 跟链 + 4 个只读 SOAP 业务方法
//! - [`Mail`] / [`MailFull`] / [`Address`] —— domain model
//!
//! 红线：见 `soap.rs` 模块文档。

pub use client::{LoginMeta, MailClient};
pub use models::{Address, Mail, MailFull};

mod client;
mod extract;
mod http;
mod models;
mod parser;
mod soap;
#[cfg(test)]
mod tests_parse;
mod throttle;
