//! `sjtu mail` 命令层数据形状：Envelope `data` 字段的 Rust 类型。
//!
//! 两个维度：
//! - 列表（list / mails）：`MailListData` 含 `Vec<MailListRow>`
//! - 单封正文（read）：`MailReadData` 含完整 body + 警告信息

use serde::Serialize;

use crate::apps::mail::{Address, Mail, MailFull};

/// `sjtu mail list` 响应体：收件箱邮件列表。
#[derive(Debug, Serialize)]
pub struct MailListData {
    /// Zimbra query 字符串（如 `in:inbox` / `is:unread` / `in:inbox "kw"`），Agent 看本次用了什么。
    pub query: String,
    /// 本次返回条数。
    pub count: usize,
    /// 分页偏移（用于 Agent 下一页）。
    pub offset: u32,
    /// 是否还有更多（count == limit 时为 true，提示下一页）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
    /// 邮件行列表。
    pub items: Vec<MailListRow>,
}

/// 列表里的单行邮件摘要（非完整正文）。
#[derive(Debug, Serialize)]
pub struct MailListRow {
    /// Zimbra 邮件 ID（传给 `sjtu mail read <id>`）。
    pub id: String,
    /// 邮件主题。
    pub subject: Option<String>,
    /// 正文预览（~200 字符）。
    pub fragment: Option<String>,
    /// 发件人邮箱。
    pub from_address: Option<String>,
    /// 发件人显示名。
    pub from_display: Option<String>,
    /// 时间戳（毫秒 unix epoch）。
    pub date_ms: Option<i64>,
    /// 字节大小。
    pub size_bytes: Option<i64>,
    /// 是否未读。
    pub unread: bool,
}

/// `sjtu mail read <id>` 响应体：单封完整邮件。
#[derive(Debug, Serialize)]
pub struct MailReadData {
    /// 邮件 ID。
    pub id: String,
    /// 主题。
    pub subject: Option<String>,
    /// 发件人邮箱。
    pub from_address: Option<String>,
    /// 发件人显示名。
    pub from_display: Option<String>,
    /// 收件人列表。
    pub to_addresses: Vec<Address>,
    /// 抄送列表。
    pub cc_addresses: Vec<Address>,
    /// 时间戳（毫秒 unix epoch）。
    pub date_ms: Option<i64>,
    /// 是否未读（meta.unread 透传；read=0 不改原状态）。
    pub unread: bool,
    /// 解析后的纯文本 body。HTML-only 邮件则为 None，见 `body_warning`。
    pub body_plain: Option<String>,
    /// HTML-only 邮件时填提示文字；有 plain text 时为 None。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_warning: Option<String>,
}

impl From<Mail> for MailListRow {
    fn from(m: Mail) -> Self {
        Self {
            id: m.id,
            subject: m.subject,
            fragment: m.fragment,
            from_address: m.from_address,
            from_display: m.from_display,
            date_ms: m.date_ms,
            size_bytes: m.size_bytes,
            unread: m.unread,
        }
    }
}

impl From<MailFull> for MailReadData {
    fn from(f: MailFull) -> Self {
        // HTML-only 邮件：body_plain=None → 填 body_warning 提示。
        let body_warning = if f.body_plain.is_none() {
            Some("此邮件无 text/plain part，body_plain 为空；HTML 正文请在邮件客户端查看。".into())
        } else {
            None
        };
        Self {
            id: f.meta.id,
            subject: f.meta.subject,
            from_address: f.meta.from_address,
            from_display: f.meta.from_display,
            to_addresses: f.to_addresses,
            cc_addresses: f.cc_addresses,
            date_ms: f.meta.date_ms,
            unread: f.meta.unread,
            body_plain: f.body_plain,
            body_warning,
        }
    }
}
