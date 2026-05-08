//! 课堂视频（v.sjtu.edu.cn）API 响应 / 请求结构体。
//!
//! 契约来自 tasks/canvas_video_investigation.md §2..§5：
//! - `getAccessTokenByTokenId`（GET）→ `GetTokenResponse`
//!   仅取 `data.token` / `data.params.courId` / `data.params.ltiCourseId`，
//!   `userCode` / `userName` / `accessToken.jwt_token` 含 PII，**不持久化、不打日志**。
//! - `findVodVideoList`（POST）请求 `FindListRequest`，响应 `FindListResponse`。
//! - 通用包装 `code/data/message/status/success/timestamp` 用 `ApiEnvelope<T>` 抽象。
//!
//! `#[serde(default)]` + `Option<T>` 抗 API 漂移；`rename_all = "camelCase"` 对齐后端。

use serde::{Deserialize, Serialize};

/// v.sjtu 通用响应包装：`code=="0" && success==true` 视作业务成功。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct ApiEnvelope<T> {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(default)]
    pub message: Option<String>,
    /// 抗 API 漂移：保留 status/timestamp 字段反序列化路径，目前 CLI 不读。
    #[allow(dead_code)]
    #[serde(default)]
    pub status: Option<i32>,
    pub data: Option<T>,
}

impl<T> ApiEnvelope<T> {
    /// 业务成功判定：`code == "0"` 且 `success == true`。两者缺失视为失败。
    pub fn is_business_ok(&self) -> bool {
        self.code.as_deref() == Some("0") && self.success.unwrap_or(false)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// getAccessTokenByTokenId
// ─────────────────────────────────────────────────────────────────────────────

/// `getAccessTokenByTokenId` 的 `data` 字段（仅取 CLI 关心的子集）。
///
/// 故意只反序列化 4 个字段：避免把 `userCode` / `userName` / `jwt_token` 等
/// PII 拽进 struct（即便后续不展示，编译产物也别带）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TokenData {
    /// 后续 API 调用的 `token:` header（HS512+GZIP），约 600 字符。
    #[serde(default)]
    pub token: Option<String>,
    pub params: Option<TokenParams>,
}

/// `data.params` 子结构。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TokenParams {
    /// RSA/AES 加密的 base64+/= 字符串，POST body 时再 URL-encode。
    #[serde(default)]
    pub cour_id: Option<String>,
    /// 32-hex 内部 LTI 课程 ID。
    #[serde(default)]
    pub lti_course_id: Option<String>,
    /// 课程全名（含工号格式）—— 含 PII，**不持久化、不打日志**；保留字段仅用于内存中
    /// 友好错误信息或 `--with-identity` 输出（CP-V1 暂未读，CP-V2 起用）。
    #[allow(dead_code)]
    #[serde(default)]
    pub course_name: Option<String>,
}

/// `getAccessTokenByTokenId` 完整响应（业务成功后组装到 Bootstrap）。
pub(super) type GetTokenResponse = ApiEnvelope<TokenData>;

// ─────────────────────────────────────────────────────────────────────────────
// findVodVideoList
// ─────────────────────────────────────────────────────────────────────────────

/// `findVodVideoList` 请求体。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FindListRequest {
    /// **URL-encoded** 后的 `data.params.courId`（`/` → `%2F` 等）。
    pub canvas_course_id: String,
}

/// `findVodVideoList.data.records[]` 的核心字段。详见调研 §5。
///
/// 默认结构暴露给 CLI 上层（commands::canvas_video）；PII 字段（`user_name` =
/// 教师真姓名，公开属性）默认输出，handlers 层按 `--with-identity` 决定是否抹。
/// 学生侧的 `play_count` / `play_time` 等观看习惯字段**不入 struct**——
/// 直接不解析，避免上层误用。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LectureVideo {
    /// 每讲视频 ID（base64 带 `=` 后缀），`getVodVideoInfos` 入参 ★。
    #[serde(default)]
    pub video_id: String,
    /// 形如 `<课程名>(第N讲)`。
    #[serde(default)]
    pub video_name: String,
    /// 上课开始 `YYYY-MM-DD HH:MM:SS`（v.sjtu 后端时区）。
    #[serde(default)]
    pub course_begin_time: Option<String>,
    /// 上课结束。
    #[serde(default)]
    pub course_end_time: Option<String>,
    /// 教室名（如 `上院210`）。
    #[serde(default)]
    pub classroom_name: Option<String>,
    /// 教师真姓名。教学公开属性可留；`--redact-teacher` 抹掉。
    #[serde(default)]
    pub user_name: Option<String>,
    /// 课次内部 ID（int），留作 join key。
    #[serde(default)]
    pub cour_id: Option<i64>,
    /// 视频审核状态（3=已审通过等）。CLI 默认 filter `==3`。
    #[serde(default)]
    pub vide_audit_status: Option<i32>,
}

/// `findVodVideoList.data` —— Spring `Page<T>` 形态。MVP 一次取 `size=2000`，
/// 不分页。`size`/`current`/`pages` 抗漂移保留，CP-V1 不读。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct FindListPage {
    #[serde(default)]
    pub records: Vec<LectureVideo>,
    #[serde(default)]
    pub total: i64,
    #[allow(dead_code)]
    #[serde(default)]
    pub size: i64,
    #[allow(dead_code)]
    #[serde(default)]
    pub current: i64,
    #[allow(dead_code)]
    #[serde(default)]
    pub pages: i64,
}

/// `findVodVideoList` 完整响应。
pub(super) type FindListResponse = ApiEnvelope<FindListPage>;

// ─────────────────────────────────────────────────────────────────────────────
// Bootstrap：跨模块传递的 LTI launch 结果
// ─────────────────────────────────────────────────────────────────────────────

/// LTI launch + token 提取的结果（CP-V1 主输出契约）。
///
/// 字段语义见 tasks/canvas_video_investigation.md §4。`token` 含 PII（jwt sub），
/// 仅在内存里用于本次 CLI 调用；**不落盘、不入 tracing 完整值**。
#[derive(Debug, Clone)]
pub struct Bootstrap {
    /// HS512 from `data.token`，后续 API `token:` header。
    pub token: String,
    /// RSA encrypted from `data.params.courId`（POST body 前再 URL-encode）。
    pub cour_id: String,
    /// 32-hex from `data.params.ltiCourseId`。
    pub lti_course_id: String,
    /// v.sjtu 域 cookie（JSESSIONID / route），仅内存。
    pub session_cookies: Vec<crate::cookies::Cookie>,
}
