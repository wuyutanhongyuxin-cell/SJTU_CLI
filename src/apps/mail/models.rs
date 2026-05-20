//! Zimbra SOAP 响应解析后的 domain model。
//!
//! 注意：这里不是 SOAP XML 反序列化的中间形，是已经过 `soap.rs::parse_*`
//! 处理后的"业务层"对象。命令层 / Envelope 直接消费这个形。

use serde::{Deserialize, Serialize};

/// 单条邮件（SearchResponse 里的 `<m>`，或 GetMsgResponse 的元数据部分）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Mail {
    /// Zimbra 邮件 ID（数字字符串，CLI 命令 `sjtu mail <id>` 的入参）。
    pub id: String,
    /// 文件夹 ID（默认 inbox = "2"）。
    #[serde(default)]
    pub folder_id: Option<String>,
    /// 会话 ID（同一对话多封共享）。
    #[serde(default)]
    pub conversation_id: Option<String>,
    /// 邮件主题（`<su>`）。
    #[serde(default)]
    pub subject: Option<String>,
    /// 正文预览（`<fr>`，~200 字符）。
    #[serde(default)]
    pub fragment: Option<String>,
    /// 发件人完整地址（`<e t="f" a="...">`）。
    #[serde(default)]
    pub from_address: Option<String>,
    /// 发件人显示名（`<e t="f" d="...">`）。
    #[serde(default)]
    pub from_display: Option<String>,
    /// 邮件时间戳（毫秒 unix epoch；`<m d="...">`）。
    #[serde(default)]
    pub date_ms: Option<i64>,
    /// 字节大小。
    #[serde(default)]
    pub size_bytes: Option<i64>,
    /// 原始 flags 字段（`f="u"`/`"r"`/`"s"` etc）。
    #[serde(default)]
    pub flags: Option<String>,
    /// 是否未读（flags 含 'u'）。
    #[serde(default)]
    pub unread: bool,
}

/// 单封邮件完整正文（GetMsgResponse 解析后）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MailFull {
    /// 复用 Mail 头部。
    #[serde(flatten)]
    pub meta: Mail,
    /// 解析后的 plain text body（`<mp ct="text/plain"><content>` 拼接）。
    /// HTML-only 邮件 → 命令层填 None + Envelope warning。
    #[serde(default)]
    pub body_plain: Option<String>,
    /// 收件人列表（`<e t="t" a="...">`）。
    #[serde(default)]
    pub to_addresses: Vec<Address>,
    /// 抄送列表（`<e t="c">`）。
    #[serde(default)]
    pub cc_addresses: Vec<Address>,
}

/// 邮件地址项（来自 `<e>` 元素）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Address {
    /// 完整邮箱（`a` 属性）。
    pub address: String,
    /// 显示名（`d` 属性）。
    #[serde(default)]
    pub display: Option<String>,
}

/// `<m>` 的 flags 字段含字符 'u' 即未读。
pub(super) fn flags_contains_unread(flags: Option<&str>) -> bool {
    flags.map(|s| s.contains('u')).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unread_detection() {
        assert!(flags_contains_unread(Some("u")));
        assert!(flags_contains_unread(Some("ur"))); // unread + replied
        assert!(!flags_contains_unread(Some("r")));
        assert!(!flags_contains_unread(None));
        assert!(!flags_contains_unread(Some("")));
    }

    #[test]
    fn mail_default_unread_false() {
        let m = Mail::default();
        assert!(!m.unread);
        assert!(m.subject.is_none());
    }

    #[test]
    fn address_serializes_minimal() {
        let a = Address {
            address: "noreply@example.com".into(),
            display: None,
        };
        let s = serde_json::to_string(&a).unwrap();
        assert!(s.contains("noreply@example.com"));
    }
}
