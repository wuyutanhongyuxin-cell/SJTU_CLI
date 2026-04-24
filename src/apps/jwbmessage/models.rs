//! 交我办消息中心响应结构体。
//!
//! API 契约来自 tasks/s3b-jiaowoban-messages.md §2：
//! - `/api/jwbmessage/unreadNum` 返 `{total, errno, ...}`
//! - `/api/jwbmessage/group` 返 `{total, entities: [Group, ...], ...}`
//! - `/api/jwbmessage/messagelist` 返 `{total, entities: [Message, ...], ...}`
//! - `/api/jwbmessage/message/readall` 未实测 → 用宽松 `ReadAllResponse`
//!
//! 字段用 `#[serde(default)]` / `Option<T>` 抗 API 漂移；关键字段（id / name）保留必填。

use serde::{Deserialize, Serialize};

/// GET /api/jwbmessage/unreadNum：`total` 就是全局未读总数。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UnreadNum {
    pub total: u32,
    #[serde(default)]
    pub errno: i32,
}

/// GET /api/jwbmessage/group 的单条分组。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub group_id: String,
    #[serde(default)]
    pub group_name: String,
    #[serde(default)]
    pub unread_num: u32,
    #[serde(default)]
    pub group_description: Option<String>,
    #[serde(default)]
    pub is_group: bool,
    /// 注意：可能与 `unread_num > 0` 共存，语义矛盾；**以 `unread_num` 为准**。
    #[serde(default)]
    pub is_read: bool,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub create_time: Option<String>,
}

/// GET /api/jwbmessage/group 顶层包装。
#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct GroupEnvelope {
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub entities: Vec<Group>,
}

/// 消息发送方（App）元数据，对应 `Message.authClient`。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthClient {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
}

/// 消息正文的结构化条目（`Message.context[]`）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextItem {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub value: String,
}

/// GET /api/jwbmessage/messagelist 的单条消息。
///
/// `push_content` 就是详情正文 —— 无单独详情端点，CLI 的 `show` 直接用本字段。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    #[serde(default)]
    pub message_id: String,
    /// `type` 是 Rust 关键字，serde rename 过来。
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub read_time: Option<String>,
    #[serde(default)]
    pub read: bool,
    #[serde(default)]
    pub expire_time: Option<String>,
    #[serde(default)]
    pub notification_id: Option<String>,
    #[serde(default)]
    pub create_time: Option<String>,
    #[serde(default)]
    pub push_title: Option<String>,
    #[serde(default)]
    pub push_content: Option<String>,
    #[serde(default)]
    pub auth_client: Option<AuthClient>,
    #[serde(default)]
    pub picture: Option<String>,
    /// 水源 API 实测可能返 null / 数组 / 对象，宽松 Value。
    #[serde(default)]
    pub url_list: Option<serde_json::Value>,
    #[serde(default)]
    pub context: Option<Vec<ContextItem>>,
}

/// GET /api/jwbmessage/messagelist 顶层包装。
#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct MessageListEnvelope {
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub entities: Vec<Message>,
}

/// POST /api/jwbmessage/message/readall 响应：未实测，宽松接住。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReadAllResponse {
    #[serde(default)]
    pub errno: i32,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub message: Option<String>,
}
