//! `sjtu canvas-video <sub>` 的数据形状。每个 `cmd_*` 对应一个 `*Data` 结构。
//!
//! 隐私策略（详见 tasks/canvas_video_investigation.md §8）：
//! - `cour_id` / `lti_course_id`：默认显示前 12 字符 + `***`，`--with-identity` 才出全文
//! - 教师 `user_name`：默认保留（教学公开属性），CLI 暂不提供 `--redact-teacher` 开关
//! - 学生侧 `play_count` / `play_time` / `last_watch_time`：根本不解析，数据流到此为止

use serde::Serialize;

/// `sjtu canvas-video list` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct ListData {
    /// Canvas 数字课程 ID（入参回显）。
    pub course_id: u64,
    /// LTI 工具 ID（入参回显）。
    pub tool_id: u64,
    /// 是否输出原始 PII。
    pub with_identity: bool,
    /// 是否包含未审核视频。
    pub include_unaudited: bool,
    /// 服务端 total（过滤前）。
    pub total_raw: i64,
    /// 客户端过滤后返回的条目数。
    pub returned: usize,
    /// 内部 cour_id / lti_course_id：默认前 12 字符 + `***`，with_identity 才出全文。
    pub cour_id_redacted: String,
    pub lti_course_id_redacted: String,
    /// 视频列表（已扁平化）。
    pub items: Vec<LectureEntry>,
}

/// 每讲视频的扁平化视图。字段语义见调研 §5。
#[derive(Debug, Serialize)]
pub(super) struct LectureEntry {
    /// 讲序号（按 course_begin_time 升序后的 1-based index；老师重传后可能错位）。
    pub seq: u32,
    /// `videoId` —— `getVodVideoInfos` 入参（CP-V3 起用）。
    pub video_id: String,
    /// `<课程名>(第N讲)`。
    pub video_name: String,
    /// 上课开始时间（v.sjtu 后端原文 `YYYY-MM-DD HH:MM:SS`，未做时区转换）。
    pub course_begin_time: Option<String>,
    /// 上课结束。
    pub course_end_time: Option<String>,
    /// 教室名（如 `上院210`）。
    pub classroom_name: Option<String>,
    /// 教师姓名（教学公开属性，默认保留）。
    pub teacher: Option<String>,
    /// 课次内部 ID。
    pub cour_id: Option<i64>,
    /// 视频审核状态（3=已审通过）。CLI 默认过滤 `==3`，传 `--include-unaudited` 才返非 3。
    pub vide_audit_status: Option<i32>,
}
