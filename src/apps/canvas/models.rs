//! Canvas REST API 响应结构体。
//!
//! 契约来自 tasks/s3c-canvas-planner.md §2：
//! - `/api/v1/users/self` + `/api/v1/users/self/profile`：合并成 `Profile`
//! - `/api/v1/planner/items`：`Vec<PlannerItem>`
//!
//! `Accept: application/json+canvas-string-ids` → 所有 id 是字符串。这里全用 `String`
//! 接，不做字符串→数字推断，避免未来 Canvas 升级 64-bit id 时溢出。

use serde::{Deserialize, Serialize};

/// `/api/v1/users/self` 的响应（只取本 CLI 关心的字段）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserSelf {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub short_name: Option<String>,
    #[serde(default)]
    pub sortable_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    /// 用户偏好语言（常为 null，看 Canvas 站点默认）。
    #[serde(default)]
    pub locale: Option<String>,
    /// 实际生效语言（SJTU Canvas 固定 `zh-Hans`）。
    #[serde(default)]
    pub effective_locale: Option<String>,
}

/// `/api/v1/users/self/profile` 的响应（与 `UserSelf` 有重叠字段）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserProfile {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub short_name: Option<String>,
    #[serde(default)]
    pub primary_email: Option<String>,
    /// 登录名 = JAccount 用户名。
    #[serde(default)]
    pub login_id: Option<String>,
    /// 时区，SJTU 固定 `Asia/Shanghai`。
    #[serde(default)]
    pub time_zone: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub effective_locale: Option<String>,
    /// 含个人 iCal feed URL（MVP 不用，留作 Phase 2 offline 路线）。
    #[serde(default)]
    pub calendar: Option<CalendarLink>,
}

/// `profile.calendar.ics` —— 个人日历 iCal 订阅 URL。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CalendarLink {
    #[serde(default)]
    pub ics: Option<String>,
}

/// 合并 `/users/self` + `/users/self/profile` 后的统一视图，Envelope 暴露给 Agent。
#[derive(Debug, Clone, Serialize, Default)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub short_name: Option<String>,
    pub login_id: Option<String>,
    pub primary_email: Option<String>,
    pub time_zone: Option<String>,
    pub effective_locale: Option<String>,
}

impl Profile {
    /// 合并两份响应。字段冲突时优先取 `profile`（更全）；`user` 做兜底。
    pub fn merge(user: UserSelf, profile: UserProfile) -> Self {
        Self {
            id: if profile.id.is_empty() {
                user.id
            } else {
                profile.id
            },
            name: if profile.name.is_empty() {
                user.name
            } else {
                profile.name
            },
            short_name: profile.short_name.or(user.short_name),
            login_id: profile.login_id,
            primary_email: profile.primary_email,
            time_zone: profile.time_zone,
            effective_locale: profile.effective_locale.or(user.effective_locale),
        }
    }
}

/// `/api/v1/planner/items` 的单条。所有时间字段都是 UTC（尾 Z）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlannerItem {
    #[serde(default)]
    pub context_type: Option<String>,
    #[serde(default)]
    pub course_id: Option<String>,
    #[serde(default)]
    pub plannable_id: String,
    /// assignment / discussion_topic / quiz / planner_note / calendar_event
    #[serde(default)]
    pub plannable_type: Option<String>,
    /// 排序主键（UTC）。作业类与 `plannable.due_at` 相等；`planner_note` 只有这个字段。
    #[serde(default)]
    pub plannable_date: Option<String>,
    #[serde(default)]
    pub new_activity: bool,
    #[serde(default)]
    pub submissions: Option<Submissions>,
    #[serde(default)]
    pub plannable: Option<Plannable>,
    #[serde(default)]
    pub html_url: Option<String>,
    /// 课程简称。MVP 直接取这个，不再单独查 dashboard_cards。
    #[serde(default)]
    pub context_name: Option<String>,
}

/// `plannable` 字段：作业 / 讨论 / 测验 的公共子集。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Plannable {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub points_possible: Option<f64>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// 交付状态。`planner_note` / `calendar_event` 没有这个字段。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Submissions {
    #[serde(default)]
    pub submitted: bool,
    #[serde(default)]
    pub excused: bool,
    #[serde(default)]
    pub graded: bool,
    #[serde(default)]
    pub late: bool,
    #[serde(default)]
    pub missing: bool,
    #[serde(default)]
    pub needs_grading: bool,
    #[serde(default)]
    pub has_feedback: bool,
    #[serde(default)]
    pub redo_request: bool,
    #[serde(default)]
    pub posted_at: Option<String>,
}

impl Submissions {
    /// CLI 语义"未完成"= 未交 且 未获免交。`planner_note` 没 submissions 时视为未完成。
    pub fn is_outstanding(opt: Option<&Submissions>) -> bool {
        match opt {
            None => true,
            Some(s) => !s.submitted && !s.excused,
        }
    }
}
