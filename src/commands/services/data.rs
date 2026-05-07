//! `sjtu services <sub>` 的数据形状。每个 `cmd_*` 对应一个 `*Data` 结构。
//!
//! 通过 Envelope<T> 序列化后暴露给 Agent。

use serde::Serialize;

use crate::apps::services::TodoItem;

/// `sjtu services pending` 的 data 形状。
///
/// **分流口径**（§6.6）：UI 端按 `code == "ADD"` 区分"我申请的（1）"vs"等我处理的（0）"，
/// CLI 端复刻这条逻辑。
#[derive(Debug, Serialize)]
pub(super) struct PendingData {
    /// 服务端返回的 total 字段（与 UI "共 N 条" 对齐）。
    pub total: u32,
    /// 客户端实际收到的条数（理论上 == total，但宽松独立暴露）。
    pub returned: usize,
    /// 是否暴露身份字段（`--with-identity` 透传）。
    pub with_identity: bool,
    /// `code == "ADD"` 的事项 —— 用户自己发起的流程。
    pub my_applications: Vec<TodoItem>,
    /// 其他 `code` 的事项 —— 等用户处理（审核 / 签字 / 等）的流程。
    pub awaiting_my_action: Vec<TodoItem>,
}
