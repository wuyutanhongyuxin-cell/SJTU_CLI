//! `sjtu canvas-video list` handler（CP-V1）。`download` 拆到 `download_handler.rs`。
//!
//! 流程：connect → findVodVideoList → 过滤已审 → course_begin_time 升序 → 1-based seq
//! → PII 字段按 `--with-identity` 展开 / 抹 → Envelope 输出。

use std::future::Future;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use crate::apps::canvas_video::{cache, Client, LectureVideo};
use crate::error::SjtuCliError;
use crate::output::{render, Envelope, OutputFormat};

use super::data::{LectureEntry, ListData};

/// `sjtu canvas-video list <course_id>`：列一门课的所有讲。
pub async fn cmd_list(
    course_id: u64,
    tool_id: u64,
    with_identity: bool,
    include_unaudited: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let client = Client::connect(course_id, tool_id).await?;
    let (raw, total_raw) = client
        .list_lectures(client.cour_id(), client.lti_course_id())
        .await?;

    // 过滤 + 排序 + 编号。
    let mut filtered: Vec<LectureVideo> = if include_unaudited {
        raw
    } else {
        raw.into_iter()
            .filter(|v| v.vide_audit_status == Some(3))
            .collect()
    };
    filtered.sort_by(|a, b| {
        a.course_begin_time
            .as_deref()
            .unwrap_or("")
            .cmp(b.course_begin_time.as_deref().unwrap_or(""))
    });
    let entries: Vec<LectureEntry> = filtered
        .into_iter()
        .enumerate()
        .map(|(i, v)| to_entry(i as u32 + 1, v))
        .collect();

    let cour_id_redacted = redact_or_full(client.cour_id(), with_identity);
    let lti_course_id_redacted = redact_or_full(client.lti_course_id(), with_identity);

    render(
        Envelope::ok(ListData {
            course_id,
            tool_id,
            with_identity,
            include_unaudited,
            total_raw,
            returned: entries.len(),
            cour_id_redacted,
            lti_course_id_redacted,
            items: entries,
        }),
        fmt,
    )
}

/// 把 `LectureVideo`（apps 层）映射到 `LectureEntry`（CLI 层）。
fn to_entry(seq: u32, v: LectureVideo) -> LectureEntry {
    LectureEntry {
        seq,
        video_id: v.video_id,
        video_name: v.video_name,
        course_begin_time: v.course_begin_time,
        course_end_time: v.course_end_time,
        classroom_name: v.classroom_name,
        teacher: v.user_name,
        cour_id: v.cour_id,
        vide_audit_status: v.vide_audit_status,
    }
}

/// `--lecture N` → 实际 `LectureVideo`：list_lectures + 过滤 vide_audit_status==3 +
/// 按 course_begin_time 升序 + nth(N-1)。download / batch handler 复用。
pub(super) async fn resolve_target(client: &Client, lecture: u32) -> Result<LectureVideo> {
    if lecture == 0 {
        return Err(SjtuCliError::InvalidInput("--lecture 从 1 起，0 无效".into()).into());
    }
    let audited = list_audited_sorted(client).await?;
    let total = audited.len();
    audited
        .into_iter()
        .nth(lecture as usize - 1)
        .ok_or_else(|| {
            SjtuCliError::InvalidInput(format!("课程仅 {total} 讲（已审），不存在第 {lecture} 讲"))
                .into()
        })
}

/// list_lectures + 过滤已审 + course_begin_time 升序，1-based 下标即第 N 讲。
pub(super) async fn list_audited_sorted(client: &Client) -> Result<Vec<LectureVideo>> {
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
    Ok(audited)
}

/// `--with-identity=true` 直出全文；否则脱敏成 `prefix(12)***`。下载 handler 复用。
pub(super) fn redact_or_full(s: &str, with_identity: bool) -> String {
    if with_identity {
        return s.to_string();
    }
    if s.len() <= 12 {
        "***".to_string()
    } else {
        format!("{}***", &s[..12])
    }
}

/// Windows-safe 文件名：禁字符 `< > : " / \ | ? *` + 控制字符 → `_`；剥首尾空格点号下划线。
pub(super) fn safe_filename(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed: String = s
        .trim_matches(|c: char| matches!(c, ' ' | '.' | '_'))
        .into();
    if trimmed.is_empty() {
        "video".into()
    } else {
        trimmed
    }
}

/// 路径绝对化失败回退到原 `to_string_lossy`（一般出现在文件还未落盘的场景）。
pub(super) fn absolutize(p: &Path) -> String {
    std::fs::canonicalize(p)
        .map(|abs| abs.to_string_lossy().to_string())
        .unwrap_or_else(|_| p.to_string_lossy().to_string())
}

/// mp4 URL 默认抹（含 `key=` 时效签名）：仅保 scheme + host。`with_identity=true` 直出全文。
pub(super) fn redact_url(url: &str, with_identity: bool) -> String {
    if with_identity {
        return url.to_string();
    }
    match url::Url::parse(url) {
        Ok(u) => format!("{}://{}/...***", u.scheme(), u.host_str().unwrap_or("?")),
        Err(_) => "***".into(),
    }
}
/// V5.A 业务失败回退：套外面跑一次 op；首次抛 token-invalid 错时清 cache 再跑一次。
/// `looks_like_token_invalid` 决定哪些错触发重试；其他错原封返。
/// T11/T12 接入：`|client| async move { client.list_lectures(...).await }`。
pub(super) async fn with_token_refresh<F, Fut, T>(
    course_id: u64,
    lti_tool_id: u64,
    op: F,
) -> Result<T>
where
    F: Fn(Arc<Client>) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let client = Arc::new(Client::connect(course_id, lti_tool_id).await?);
    match op(client.clone()).await {
        Ok(v) => Ok(v),
        Err(e) if looks_like_token_invalid(&e) => {
            tracing::warn!(course_id, lti_tool_id, error = %e, "token 疑作废，清 cache 重试");
            cache::clear(Some(course_id), Some(lti_tool_id))?;
            let client2 = Arc::new(Client::connect(course_id, lti_tool_id).await?);
            op(client2).await
        }
        Err(e) => Err(e),
    }
}
/// token 疑似失效的错误特征。宁宽勿严（误判多一次 ~21s launch；漏判用户报错）。
fn looks_like_token_invalid(e: &anyhow::Error) -> bool {
    let s = e.to_string();
    s.contains("业务失败 code")
        || s.contains("401")
        || s.contains("403")
        || s.contains("未授权")
}
