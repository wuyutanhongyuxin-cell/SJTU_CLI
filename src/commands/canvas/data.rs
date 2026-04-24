//! `sjtu canvas <sub>` 的数据形状。每个 `cmd_*` 对应一个 `*Data` 结构。
//!
//! 从 handlers 拆出守 200 行硬限；通过 Envelope<T> 序列化后暴露给 Agent。

use serde::Serialize;

use crate::apps::canvas::Profile;

/// `sjtu canvas setup` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct SetupData {
    /// PAT 文件落盘路径（展示给用户，不含 token 内容）。
    pub token_path: String,
    /// 跑过 whoami 验证后取到的 login_id，空表示未验证。
    pub login_id: Option<String>,
}

/// `sjtu canvas whoami` 的 data 形状。直接就是 `Profile`。
pub(super) type WhoamiData = Profile;

/// planner 条目的 CLI 视图。扁平化 + 本地化，便于 Agent 消费。
#[derive(Debug, Serialize)]
pub(super) struct PlannerEntry {
    /// assignment / discussion_topic / quiz / planner_note / calendar_event
    pub kind: Option<String>,
    pub title: Option<String>,
    pub course: Option<String>,
    pub course_id: Option<String>,
    pub plannable_id: String,
    /// 原始 UTC 时间（ISO8601 尾 Z），便于 Agent 做相对时间计算。
    pub due_at_utc: Option<String>,
    /// 本地化时间（Asia/Shanghai，格式 `YYYY-MM-DD HH:MM`），便于人眼快速扫。
    pub due_at_local: Option<String>,
    /// `"75 分"` / `"不计分"` / null。
    pub points: Option<String>,
    pub submitted: bool,
    pub missing: bool,
    pub graded: bool,
    pub html_url: Option<String>,
}

/// `sjtu canvas today` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct TodayData {
    /// 查询窗口（本地日期，`YYYY-MM-DD`）。
    pub date_local: String,
    /// 是否包含已完成。
    pub include_done: bool,
    /// 客户端过滤后返回的条目数。
    pub returned: usize,
    /// 服务端返回的原始总条数（过滤前）。
    pub total_raw: usize,
    pub items: Vec<PlannerEntry>,
}

/// `sjtu canvas upcoming` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct UpcomingData {
    pub days: u32,
    pub start_local: String,
    pub end_local: String,
    pub include_done: bool,
    pub returned: usize,
    pub total_raw: usize,
    pub items: Vec<PlannerEntry>,
}
