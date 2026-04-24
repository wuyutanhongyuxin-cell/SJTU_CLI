//! `sjtu messages <sub>` 的数据形状。每个 `cmd_*` 对应一个 `*Data` 结构。
//!
//! 从 handlers 拆出守 200 行硬限；通过 Envelope<T> 序列化后暴露给 Agent。

use serde::Serialize;

use crate::apps::jwbmessage::{Group, Message, ReadAllResponse};

/// `sjtu messages list` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct ListData {
    pub page: u32,
    pub unread_only: bool,
    /// 客户端过滤后实际返回的分组数。
    pub returned: usize,
    /// 服务端 total 字段（未过滤前的全量）。
    pub total: u32,
    pub groups: Vec<Group>,
}

/// `sjtu messages show <group-id>` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct ShowData {
    pub group_id: String,
    pub is_group: bool,
    pub include_read: bool,
    pub returned: usize,
    pub total: u32,
    /// **隐式副作用警示**：该组所有未读已被服务端标记为已读。
    pub side_effect_marked_read: bool,
    pub messages: Vec<Message>,
}

/// `sjtu messages read-all` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct ReadAllData {
    pub marked: bool,
    pub response: ReadAllResponse,
}
