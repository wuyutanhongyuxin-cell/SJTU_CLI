//! 办事大厅响应结构体。
//!
//! 契约来自 tasks/isjtu_investigation.md §6.3 N-todo（CLI MVP 仅用待办）。
//!
//! **schema 关键差异**（§6.6 已知坑）：
//! - todo 端点的 entity = 当前步骤铺顶层 + 嵌套 `process` 子对象
//! - completed / cc 端点的 entity 是铺平的（没有 `process` 子对象）
//!
//! MVP 只覆盖 todo，因此本文件**只有 `TodoItem`**。
//! 未来 phase-2 接 completed / cc 时再加 `CompletedItem` —— **不要**强行合并。
//!
//! 大量字段用 `Option<>` + `#[serde(default)]` 抗 API 漂移：
//! 真实抓包里 `entry / owner.name / owner.id / milestone / app.code / app.contact`
//! 全可缺失（特别是通知服务平台类项）。

use serde::{Deserialize, Serialize};

/// `/api/task/me/processes/todo` 顶层包装。
///
/// `errno` / `error` 字段服务端会返但 CLI 当前不读 —— 业务错误目前靠 HTTP 状态码兜底；
/// 真观察到 `errno != 0` 的 case 再加专门处理。同 jwbmessage::GroupEnvelope 风格。
#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct TodoEnvelope {
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub entities: Vec<TodoItem>,
}

/// 待办列表里的单条事项。**当前步骤字段铺在顶层**，流程主体在 `process` 子对象里。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    /// 当前步骤 UUID（与 `process.id` 不同）。
    pub id: String,
    /// 当前步骤名（"填写申请" / "审核" / ...）。
    #[serde(default)]
    pub name: Option<String>,
    /// 步骤代码：**`"ADD"` = 我申请的；其他 = 等我处理的**。CLI 端按此分流。
    #[serde(default)]
    pub code: Option<String>,
    /// 当前步骤跳转 URL（form.sjtu.edu.cn）。
    #[serde(default)]
    pub uri: Option<String>,
    /// 派发时间（unix 秒）。
    #[serde(default)]
    pub assign_time: Option<i64>,
    /// 状态码（int，具体语义未反查）。
    #[serde(default)]
    pub status: Option<i32>,
    /// 流程主体。极少数情况下可能缺失，宽松 `Option`。
    #[serde(default)]
    pub process: Option<Process>,
}

/// 流程实例信息。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Process {
    /// 流程实例 UUID。
    #[serde(default)]
    pub id: Option<String>,
    /// 流水号（`"20054472"`，可能为空字符串）。
    #[serde(default)]
    pub entry: Option<String>,
    /// 事项标题。
    #[serde(default)]
    pub name: Option<String>,
    /// 流程详情 URL。
    #[serde(default)]
    pub uri: Option<String>,
    /// 创建时间（unix 秒）。
    #[serde(default)]
    pub create: Option<i64>,
    /// 更新时间（unix 秒）。
    #[serde(default)]
    pub update: Option<i64>,
    /// `"doing"` / `"done"` / ...
    #[serde(default)]
    pub status: Option<String>,
    /// 应用元信息。
    #[serde(default)]
    pub app: Option<App>,
    /// 申请人。**身份字段，CLI 默认脱敏**。
    #[serde(default)]
    pub owner: Option<Owner>,
    /// 进度信息。
    #[serde(default)]
    pub milestone: Option<Milestone>,
    /// 流程模板版本（unix 秒）。
    #[serde(default)]
    pub version: Option<i64>,
}

/// 应用元信息（`process.app`）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct App {
    /// 应用 code（"HXBDSQ"）。
    #[serde(default)]
    pub code: Option<String>,
    /// 应用名。
    #[serde(default)]
    pub name: Option<String>,
    /// 发起部门。
    #[serde(default)]
    pub department: Option<String>,
    /// 联系信息（电话 / 邮箱混排）。
    #[serde(default)]
    pub contact: Option<String>,
    /// 标签字符串：业务标签 + `#CategoryName:xxx,#CategoryOrder:N,#CategoryColumns:M`
    /// 显示元信息混在一起，逗号分隔。CLI **MVP 透传原始字符串**，phase-2 再解析过滤。
    #[serde(default)]
    pub tags: Option<String>,
    /// 系统名（通知服务平台类项才有）。
    #[serde(default)]
    pub system: Option<String>,
    /// UI 是否可见（false = 通知服务平台类项）。
    #[serde(default)]
    pub visible: Option<bool>,
}

/// 申请人信息（`process.owner`）。**`name` / `id` 默认脱敏，CLI handler 层置 `***`**。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Owner {
    /// jaccount 账号（不脱敏 —— 与可见 URL 一致）。
    #[serde(default)]
    pub account: Option<String>,
    /// 姓名（**身份字段，默认脱敏**）。
    #[serde(default)]
    pub name: Option<String>,
    /// 校内 UUID（**身份字段，默认脱敏**）。
    #[serde(default)]
    pub id: Option<String>,
}

/// 流程进度（`process.milestone`）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Milestone {
    /// 进度百分比（0..100）。
    #[serde(default)]
    pub percent: Option<i32>,
}
