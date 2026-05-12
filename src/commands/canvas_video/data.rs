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

/// `sjtu canvas-video download` 的 data 形状。文件路径绝对化；mp4 URL 含时效签名（仅
/// `--with-identity` 时回显，否则抹）。
#[derive(Debug, Serialize)]
pub(super) struct DownloadData {
    pub course_id: u64,
    pub tool_id: u64,
    /// 1-based 讲序号（用户输入回显）。
    pub lecture: u32,
    /// 选定的 `cdviChannelNum`（0 老师 / 1 PPT）。
    pub channel: i32,
    /// 视频名（`<课程名>(第N讲)`）。可能 None（极少见）。
    pub video_name: Option<String>,
    /// `videoId`（base64 含 `=`）：默认前 12 字符 + `***`，with_identity 才出全文。
    pub video_id_redacted: String,
    /// 视频时长（秒），来自 `videPlayTime`。
    pub duration_secs: Option<i64>,
    /// 落盘路径（绝对路径）。`audio_only && !keep_mp4` 时已被删，仅作历史标记。
    pub file_path: String,
    /// `--audio-only` 时抽出的 m4a 路径。普通下载为 None。
    pub audio_path: Option<String>,
    /// `audio_only && !keep_mp4` 时为 false（mp4 已删），其他场景 true。
    pub mp4_kept: bool,
    /// 实际写入字节数（mp4，抽完音频后即使删 mp4 这里也保留原始大小）。
    pub bytes: u64,
    /// 总耗时（毫秒；含 mp4 下载 + 可选 ffmpeg 抽流）。
    pub elapsed_ms: u128,
    /// mp4 URL：默认抹掉（含 `key=` 时效签名 + 服务器内部路径），with_identity 才出全文。
    pub mp4_url_redacted: String,
    /// 下载入口标识。取值见 ChannelOutput.download_kind。
    pub download_kind: String,
    /// 实际从 CDN 拉的字节数。mp4-full 路径下等于 `bytes`。
    pub bytes_downloaded: u64,
}

/// `sjtu canvas-video download --all-channels` 的 data 形状：双机位顺序下载，
/// 共享课程级元数据，每路一条 `ChannelOutput`。
#[derive(Debug, Serialize)]
pub(super) struct DownloadAllData {
    pub course_id: u64,
    pub tool_id: u64,
    pub lecture: u32,
    pub video_name: Option<String>,
    pub video_id_redacted: String,
    pub duration_secs: Option<i64>,
    /// 每个机位一条记录（顺序下载，channel 0 在前 / channel 1 在后）。
    pub channels: Vec<ChannelOutput>,
    /// 两路总字节。
    pub total_bytes: u64,
    /// 两路总耗时（毫秒）。
    pub total_elapsed_ms: u128,
}

/// `--all-channels` 模式下每路的输出，也复用于 batch 模式每讲每机位。
#[derive(Debug, Serialize)]
pub(super) struct ChannelOutput {
    pub channel: i32,
    pub file_path: String,
    /// `--audio-only` 时抽出的 m4a 路径。
    pub audio_path: Option<String>,
    /// `audio_only && !keep_mp4` 时为 false。
    pub mp4_kept: bool,
    /// 单一文件主产物字节数（mp4 大小，或 audio_only && !keep_mp4 模式下 mp4 删前的原始大小）。
    pub bytes: u64,
    pub elapsed_ms: u128,
    pub mp4_url_redacted: String,
    /// 下载入口标识。
    /// `mp4-full` = download.rs 全下 mp4，可选 ffmpeg 抽流
    /// `skipped` = batch 模式 dest 已存在
    pub download_kind: String,
    /// 实际从 CDN 拉的字节数。mp4-full 路径下等于 `bytes`。
    pub bytes_downloaded: u64,
}

/// `sjtu canvas-video download --lectures <SPEC>` 批量下载的 data 形状。
/// fail-soft：单讲失败进 `items[].error`，不阻塞后续；末尾 `failed_count` 给摘要。
#[derive(Debug, Serialize)]
pub(super) struct BatchData {
    pub course_id: u64,
    pub tool_id: u64,
    /// 用户原始 spec 回显（未展开）。
    pub lectures_spec: String,
    /// 双机位 / 单机位标志（用户传入）。
    pub all_channels: bool,
    /// `--audio-only` 标志。
    pub audio_only: bool,
    /// 计划下载的讲数（`parse_lectures_spec` 展开后长度）。
    pub total_planned: usize,
    /// 完整成功的讲数（每路都 ok）。
    pub succeeded: usize,
    /// 失败的讲数（任一路失败即计入）。
    pub failed_count: usize,
    /// 跳过的讲数（dest 已存在且 size 一致）。
    pub skipped: usize,
    /// 所有路总字节。
    pub total_bytes: u64,
    /// 总耗时（含 ffmpeg）。
    pub total_elapsed_ms: u128,
    /// 批量下载从 CDN 实际拉的字节累计。等价 sum(items[].channels[].bytes_downloaded)。
    pub total_bytes_downloaded: u64,
    /// 每讲一条。顺序按展开后讲序。
    pub items: Vec<BatchEntry>,
}

/// 批量模式下每讲的执行结果。
#[derive(Debug, Serialize)]
pub(super) struct BatchEntry {
    /// 1-based 讲序号。
    pub seq: u32,
    pub video_name: Option<String>,
    pub video_id_redacted: String,
    /// `ok` / `partial` / `failed` / `skipped`。`partial` = 多机位时部分路成功。
    pub status: String,
    /// 该讲的所有路输出（单机位时长度 1）。
    pub channels: Vec<ChannelOutput>,
    /// fail-soft 错误摘要：每路失败一条，文案脱去内部路径。
    pub errors: Vec<String>,
}

/// `sjtu canvas-video clear-cache` 的 envelope data。
#[derive(Debug, Serialize)]
pub(super) struct ClearCacheData {
    /// 实际删除的缓存文件数。
    pub cleared_count: u64,
    /// 范围摘要：`all` 或 `course:<id>`。
    pub scope: String,
}
