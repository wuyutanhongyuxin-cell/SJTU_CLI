//! `sjtu canvas-video download` handler（CP-V3）：单讲下载。
//!
//! 流程：connect → list_lectures → 过滤已审 + 排序 → 取第 N 讲 → get_video_info →
//! download_to_file（Range 分片并发）→ Envelope。文件名按 `<videoName>_ch<N>.mp4`
//! 生成（Windows-safe）；目录不存在自动 `create_dir_all`。

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;

use crate::apps::canvas_video::{download::download_to_file, Client, LectureVideo};
use crate::error::SjtuCliError;
use crate::output::{render, Envelope, OutputFormat};

use super::data::DownloadData;
use super::handlers::redact_or_full;

/// 下载 mp4 必带的 Referer（实测 SJTU CDN 不带就 403）。
const DOWNLOAD_REFERER: &str = "https://courses.sjtu.edu.cn";

/// `sjtu canvas-video download <course_id> --lecture N --to <dir>`：单讲落盘。
#[allow(clippy::too_many_arguments)]
pub async fn cmd_download(
    course_id: u64,
    tool_id: u64,
    lecture: u32,
    to_dir: PathBuf,
    channel: i32,
    concurrency: usize,
    with_identity: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    if lecture == 0 {
        return Err(SjtuCliError::InvalidInput("--lecture 从 1 起，0 无效".into()).into());
    }
    let started = Instant::now();
    let client = Client::connect(course_id, tool_id).await?;
    let (raw, _) = client
        .list_lectures(client.cour_id(), client.lti_course_id())
        .await?;
    let mut audited: Vec<LectureVideo> = raw
        .into_iter()
        .filter(|v| v.vide_audit_status == Some(3))
        .collect();
    audited.sort_by(|a, b| {
        a.course_begin_time
            .as_deref()
            .unwrap_or("")
            .cmp(b.course_begin_time.as_deref().unwrap_or(""))
    });
    let total = audited.len();
    let target = audited
        .into_iter()
        .nth(lecture as usize - 1)
        .ok_or_else(|| {
            SjtuCliError::InvalidInput(format!("课程仅 {total} 讲（已审），不存在第 {lecture} 讲"))
        })?;
    let fetch = client.get_video_info(&target.video_id, channel).await?;

    tokio::fs::create_dir_all(&to_dir)
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("mkdir {}: {e}", to_dir.display())))?;
    let stem = target.video_name.as_str().trim();
    let safe_stem = safe_filename(if stem.is_empty() { "video" } else { stem });
    let filename = format!("{safe_stem}_ch{}.mp4", fetch.channel);
    let dest = to_dir.join(&filename);

    let bytes = download_to_file(&fetch.mp4_url, &dest, concurrency, DOWNLOAD_REFERER).await?;

    render(
        Envelope::ok(DownloadData {
            course_id,
            tool_id,
            lecture,
            channel: fetch.channel,
            video_name: fetch.video_name.or(Some(target.video_name)),
            video_id_redacted: redact_or_full(&target.video_id, with_identity),
            duration_secs: fetch.duration_secs,
            file_path: absolutize(&dest),
            bytes,
            elapsed_ms: started.elapsed().as_millis(),
            mp4_url_redacted: redact_url(&fetch.mp4_url, with_identity),
        }),
        fmt,
    )
}

/// Windows-safe 文件名：禁字符 `< > : " / \ | ? *` + 控制字符 → `_`；剥首尾空格点号；
/// 中文括号 `（）`、书名号合法保留。
fn safe_filename(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    while matches!(s.chars().last(), Some('.' | ' ' | '_')) {
        s.pop();
    }
    while matches!(s.chars().next(), Some(' ' | '.')) {
        s.remove(0);
    }
    if s.is_empty() {
        "video".into()
    } else {
        s
    }
}

fn absolutize(p: &Path) -> String {
    std::fs::canonicalize(p)
        .map(|abs| abs.to_string_lossy().to_string())
        .unwrap_or_else(|_| p.to_string_lossy().to_string())
}

/// mp4 URL 默认抹（含 `key=` 时效签名）：仅保 scheme + host。
fn redact_url(url: &str, with_identity: bool) -> String {
    if with_identity {
        return url.to_string();
    }
    match url::Url::parse(url) {
        Ok(u) => format!("{}://{}/...***", u.scheme(), u.host_str().unwrap_or("?")),
        Err(_) => "***".into(),
    }
}
