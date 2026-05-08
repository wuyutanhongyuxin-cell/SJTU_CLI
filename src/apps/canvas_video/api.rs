//! 课堂视频 API Client：connect（拿 Bootstrap）+ list_lectures。
//!
//! 与 jwc / jwbmessage 不同：本 Client 一次性消费一个 Bootstrap（token + courId），
//! **不缓存到磁盘**（token TTL 1-3 小时，每次命令都重做 LTI launch 较稳）。
//!
//! CP-V1 仅实装 `list_lectures`。`get_video_infos` / `download_*` 留给 CP-V3。

use std::sync::Arc;

use anyhow::Result;
use reqwest::Client as HttpClient;

use super::api_form::post_form;
use super::auth::{lti_launch, url_encode_component};
use super::http::{build_http_client, post_json, BASE};
use super::models::{Bootstrap, FindListPage, FindListRequest, FindListResponse, LectureVideo};
use super::models_video::{GetVideoInfosResponse, VideoInfoData};
use super::throttle::Throttle;
use crate::error::SjtuCliError;

/// `get_video_info` 抽出的下载契约。
#[derive(Debug, Clone)]
pub struct VideoFetch {
    /// 选定机位的 mp4 直链（含 `key=` 时效签名）。
    pub mp4_url: String,
    /// 视频名（来自 `videName`，与 list 的 `videoName` 同值；可能 None）。
    pub video_name: Option<String>,
    /// 视频时长秒（None 表示未提供）。
    pub duration_secs: Option<i64>,
    /// 选中的 `cdviChannelNum`（0=老师 / 1=PPT）。
    pub channel: i32,
}

/// 课堂视频 Client。`Bootstrap` 在 `connect()` 里生成，握在结构体里供后续 API 调用。
pub struct Client {
    pub(super) http: HttpClient,
    pub(super) throttle: Arc<Throttle>,
    pub(super) bootstrap: Bootstrap,
}

impl Client {
    /// 跑一次 LTI launch（cas_login 给 oc 签发 session → headless chrome 拿 tokenId →
    /// reqwest 换 token） + 注入 v.sjtu cookie 的 reqwest client。
    ///
    /// `lti_tool_id` 默认 8329（"课堂视频new"）。主 session 由内部 cas_login 读磁盘。
    /// 失败时上抛 `UpstreamError`，上层在 envelope 里渲染。
    pub async fn connect(course_id: u64, lti_tool_id: u64) -> Result<Self> {
        let bootstrap = lti_launch(course_id, lti_tool_id).await?;
        let http = build_http_client(&bootstrap.session_cookies)?;
        Ok(Self {
            http,
            throttle: Arc::new(Throttle::new()),
            bootstrap,
        })
    }

    /// `POST /jy-application-canvas-sjtu/directOnDemandPlay/findVodVideoList`。
    ///
    /// 入参 `canvas_course_id`：直接用 `bootstrap.cour_id`（自动 URL-encode）；
    /// 入参 `_lti_course_id`：CP-V1 暂不用，保留参数槽位避免后续 CP-V2 改签名。
    /// 返回 `(LectureVideo[], total)`：MVP 取 `size=2000` 单页够用。
    pub async fn list_lectures(
        &self,
        canvas_course_id: &str,
        _lti_course_id: &str,
    ) -> Result<(Vec<LectureVideo>, i64)> {
        let body = FindListRequest {
            canvas_course_id: url_encode_component(canvas_course_id),
        };
        let body_json = serde_json::to_string(&body)
            .map_err(|e| SjtuCliError::UpstreamError(format!("序列化 list 请求失败: {e}")))?;
        let url = format!("{BASE}/jy-application-canvas-sjtu/directOnDemandPlay/findVodVideoList");
        let resp: FindListResponse = post_json(
            &self.http,
            &self.throttle,
            &url,
            &self.bootstrap.token,
            &body_json,
            "/findVodVideoList",
        )
        .await?;
        if !resp.is_business_ok() {
            return Err(SjtuCliError::UpstreamError(format!(
                "findVodVideoList 业务失败 code={:?} msg={:?}",
                resp.code, resp.message
            ))
            .into());
        }
        let page: FindListPage = resp.data.unwrap_or_default();
        Ok((page.records, page.total))
    }

    /// `POST /jy-application-canvas-sjtu/directOnDemandPlay/getVodVideoInfos`（urlencoded）。
    ///
    /// 入参：
    /// - `video_id`：来自 `LectureVideo.video_id`，原文带 `=` 后缀（自动 URL-encode）。
    /// - `channel`：`cdviChannelNum`，0=老师正面 / 1=PPT。0 找不到时回退第一条。
    ///
    /// 返回 `VideoFetch { mp4_url, video_name, duration, channel }`。失败上抛 UpstreamError。
    pub async fn get_video_info(&self, video_id: &str, channel: i32) -> Result<VideoFetch> {
        let body = format!(
            "playTypeHls=true&isAudit=true&id={}",
            url_encode_component(video_id)
        );
        let url = format!("{BASE}/jy-application-canvas-sjtu/directOnDemandPlay/getVodVideoInfos");
        let resp: GetVideoInfosResponse = post_form(
            &self.http,
            &self.throttle,
            &url,
            &self.bootstrap.token,
            &body,
            "/getVodVideoInfos",
        )
        .await?;
        if !resp.is_business_ok() {
            return Err(SjtuCliError::UpstreamError(format!(
                "getVodVideoInfos 业务失败 code={:?} msg={:?}",
                resp.code, resp.message
            ))
            .into());
        }
        let data: VideoInfoData = resp.data.unwrap_or_default();
        pick_channel(data, channel)
    }

    /// 暴露 cour_id / lti_course_id 给上层 handler，便于 envelope 输出 `--with-identity`。
    pub fn cour_id(&self) -> &str {
        &self.bootstrap.cour_id
    }
    pub fn lti_course_id(&self) -> &str {
        &self.bootstrap.lti_course_id
    }
}

/// 在多机位列表里挑一路：先按 `cdviChannelNum==channel` 精确匹配，找不到回退首条；
/// 都没有就用顶层 `rtmpUrlHdv`。三种都没 mp4 URL 即 UpstreamError。
fn pick_channel(d: VideoInfoData, channel: i32) -> Result<VideoFetch> {
    let exact = d
        .video_play_response_vo_list
        .iter()
        .find(|v| v.cdvi_channel_num == Some(channel))
        .and_then(|v| v.rtmp_url_hdv.clone().map(|u| (u, channel)));
    let fallback = d.video_play_response_vo_list.first().and_then(|v| {
        v.rtmp_url_hdv
            .clone()
            .map(|u| (u, v.cdvi_channel_num.unwrap_or(channel)))
    });
    let top = d.rtmp_url_hdv.clone().map(|u| (u, channel));
    let (mp4_url, picked) = exact
        .or(fallback)
        .or(top)
        .ok_or_else(|| SjtuCliError::UpstreamError("getVodVideoInfos 响应无可用 mp4 URL".into()))?;
    Ok(VideoFetch {
        mp4_url,
        video_name: d.vide_name,
        duration_secs: d.vide_play_time,
        channel: picked,
    })
}
