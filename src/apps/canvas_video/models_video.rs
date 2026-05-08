//! CP-V3 `getVodVideoInfos` 响应结构体。
//!
//! 仅取 mp4 URL + 多机位 + 时长 + 视频名。其余字段（含 `userCode`/`userName`/
//! `organizationCode`/`lastWatchTime` 等 PII）**故意不入 struct**：即便后端返回
//! 了，serde 也不会反序列化出来，编译产物里也不带。
//!
//! 契约源：tasks/canvas_video_investigation.md §6。

use serde::Deserialize;

use super::models::ApiEnvelope;

/// `getVodVideoInfos.data` 的 CLI 关心子集。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct VideoInfoData {
    /// 默认机位 mp4 直链（与 `videoPlayResponseVoList[0].rtmpUrlHdv` 等价）。
    #[serde(default)]
    pub rtmp_url_hdv: Option<String>,
    /// 多机位（`cdviChannelNum=0` 老师正面 / `=1` PPT），通常 2 路。
    #[serde(default)]
    pub video_play_response_vo_list: Vec<VideoPlayResponseVo>,
    /// 视频时长（秒）。
    #[serde(default)]
    pub vide_play_time: Option<i64>,
    /// 视频名（`<课程名>(第N讲)`，来自 `videName`，与 list 里的 `videoName` 同值）。
    #[serde(default)]
    pub vide_name: Option<String>,
}

/// 多机位条目。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct VideoPlayResponseVo {
    /// 该机位 mp4 直链。
    #[serde(default)]
    pub rtmp_url_hdv: Option<String>,
    /// 机位号：0=老师 / 1=PPT，可能其他值（极少见）。
    #[serde(default)]
    pub cdvi_channel_num: Option<i32>,
}

/// `getVodVideoInfos` 完整响应。`code=="0" && success==true` 视作成功。
pub(super) type GetVideoInfosResponse = ApiEnvelope<VideoInfoData>;
