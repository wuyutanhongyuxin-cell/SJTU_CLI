//! `sjtu mail` 命令集。

pub mod data;
pub mod handlers;

pub use handlers::{cmd_mail_read, cmd_mails, cmd_mails_search, cmd_mails_unread, DEFAULT_LIMIT};
