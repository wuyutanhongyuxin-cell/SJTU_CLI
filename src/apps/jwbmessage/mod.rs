//! 交我办消息中心（my.sjtu.edu.cn）客户端。
//!
//! 职责：
//! - CAS 跳转到 `/ui/app/` 注入 JSESSIONID / keepalive / PORTAL_LOCALE cookie
//! - 封装 `/api/jwbmessage/{unreadNum,group,messagelist}` 只读端点
//! - 写端点仅 `POST /api/jwbmessage/message/readall`（全部已读，全局 all-or-nothing）
//!
//! 路径契约：tasks/s3b-jiaowoban-messages.md。

mod api;
mod api_write;
mod http;
mod models;
#[cfg(test)]
mod tests_parse;
#[cfg(test)]
mod tests_write;
mod throttle;

pub use api::{Client, LoginMeta};
pub use models::{AuthClient, ContextItem, Group, Message, ReadAllResponse, UnreadNum};
