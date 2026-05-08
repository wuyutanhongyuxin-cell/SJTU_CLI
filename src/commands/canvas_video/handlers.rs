//! `sjtu canvas-video <sub>` handler：list（CP-V1）。
//!
//! 流程：
//! 1. 跑一次 LTI launch（`Client::connect` 内部 `cas_login` + headless chrome）拿 Bootstrap
//!    （`cas_login` 自己读主 session，未登录时抛"请先 sjtu login"）
//! 2. 调 `findVodVideoList` 拿 16 讲
//! 3. 默认 filter `vide_audit_status == 3`（已审）
//! 4. 按 `course_begin_time` 升序排序后给 1-based seq
//! 5. PII 字段按 `--with-identity` 展开 / 抹掉
//! 6. Envelope 输出

use anyhow::Result;

use crate::apps::canvas_video::{Client, LectureVideo};
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

/// `--with-identity=true` 直出全文；否则脱敏成 `prefix(12)***`。
fn redact_or_full(s: &str, with_identity: bool) -> String {
    if with_identity {
        return s.to_string();
    }
    if s.len() <= 12 {
        "***".to_string()
    } else {
        format!("{}***", &s[..12])
    }
}
