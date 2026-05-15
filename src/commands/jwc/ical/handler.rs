//! `sjtu jwc calendar` 子命令 handler。
//!
//! 数据流：CLI args → 默认 xnm/xqm → 并行 3 fetch（课表/考试/校历）
//! → fail-soft warning Vec → unify IcsEvent → recurrence explode
//! → RFC 5545 .ics emit → stdout / --to 文件 / Envelope JSON/YAML 双模。
//!
//! emit helpers（emit_ics / emit_one / bump_kind 等）拆到 `super::emit`，
//! 以维持本文件 < 200 行（宪法限制）。

use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;

use crate::apps::jwc::{
    default_xnm_xqm_by_date, load_from_fixture, AcademicCalendar, Client, Exam, KbItem, Schedule,
};
use crate::output::{render, Envelope, OutputFormat};

use super::emit::emit_ics;
use super::emit::ics_hash_hex;
use super::events::{from_academic, from_exam, from_kb_item, term_first_monday, IcsEvent};

// ── Envelope data structs ────────────────────────────────────────────────────

/// Envelope `data` 字段：calendar 导出摘要。
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CalendarData {
    pub xnm: String,
    pub xqm: String,
    pub event_count: usize,
    pub by_kind: ByKind,
    /// FNV-1a 16 字符 hex —— 用于 Envelope 标识，非密码学用途。
    pub hash_hex: String,
    /// .ics 字节数。
    pub bytes: usize,
    pub warnings: Vec<String>,
}

/// 三类事件计数分类。
#[derive(Debug, Serialize, Default)]
pub struct ByKind {
    pub class: usize,
    pub exam: usize,
    pub academic: usize,
}

// ── 公开入口 ─────────────────────────────────────────────────────────────────

/// `sjtu jwc calendar` 入口：并行 fetch → emit .ics → stdout 或 Envelope。
pub async fn cmd_calendar(
    client: &Client,
    xnm: Option<String>,
    xqm: Option<String>,
    to: Option<PathBuf>,
    no_academic: bool,
    no_exams: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let (xnm, xqm) = resolve_term(xnm, xqm);
    let mut warnings: Vec<String> = vec![];

    let (schedule, exams, academic) =
        fetch_all(client, &xnm, &xqm, no_exams, no_academic, &mut warnings).await?;

    let term_start = academic
        .xqkssj
        .as_deref()
        .and_then(term_first_monday)
        .unwrap_or_else(|| chrono::Local::now().date_naive());

    let all_events = unify_events(&schedule.kb_list, &exams, &academic, &xnm, &xqm, term_start);
    let (bytes, by_kind, total_count) = emit_ics(&all_events, &xnm, &xqm);
    let hash_hex = ics_hash_hex(&bytes);

    if let Some(path) = to.as_ref() {
        std::fs::write(path, &bytes)?;
    }

    // --json / --yaml 或 --to 时输出 Envelope；否则原始 .ics 到 stdout。
    let use_envelope =
        matches!(fmt, Some(OutputFormat::Json) | Some(OutputFormat::Yaml)) || to.is_some();
    if use_envelope {
        let data = CalendarData {
            xnm,
            xqm,
            event_count: total_count,
            by_kind,
            hash_hex,
            bytes: bytes.len(),
            warnings,
        };
        render(Envelope::ok(data), fmt)
    } else {
        use std::io::Write;
        std::io::stdout().write_all(&bytes)?;
        Ok(())
    }
}

// ── 私有 helpers ─────────────────────────────────────────────────────────────

/// 解析或推断学期（xnm, xqm）。
fn resolve_term(xnm: Option<String>, xqm: Option<String>) -> (String, String) {
    match (xnm, xqm) {
        (Some(a), Some(b)) => (a, b),
        _ => default_xnm_xqm_by_date(chrono::Local::now().date_naive()),
    }
}

/// 并行 fetch 课表 / 考试 / 校历；SubSessionStale 直接上抛触发 retry；其余 fail-soft。
/// 关键：stale 必须重 raise variant（非 string format），否则 downcast 链断裂 retry 失效。
async fn fetch_all(
    client: &Client,
    xnm: &str,
    xqm: &str,
    no_exams: bool,
    no_academic: bool,
    warnings: &mut Vec<String>,
) -> anyhow::Result<(Schedule, Vec<Exam>, AcademicCalendar)> {
    let class_fut = client.schedule(Some(xnm), Some(xqm));
    let exam_fut = async {
        if no_exams {
            Ok::<Vec<Exam>, anyhow::Error>(vec![])
        } else {
            client
                .exams(Some(xnm), Some(xqm), 1, 500)
                .await
                .map(|p| p.items)
        }
    };
    let academic_fut = async {
        if no_academic {
            Ok(AcademicCalendar::default())
        } else {
            let xnm_owned = xnm.to_string();
            let xqm_owned = xqm.to_string();
            match tokio::task::spawn_blocking(move || load_from_fixture(&xnm_owned, &xqm_owned))
                .await
            {
                Ok(r) => r,
                Err(e) => Err(anyhow::anyhow!("spawn_blocking: {e}")),
            }
        }
    };
    let (c_r, e_r, a_r) = tokio::join!(class_fut, exam_fut, academic_fut);

    // 任一路 SubSessionStale 立即重 raise variant，触发 with_cas_refresh retry。
    for err in [c_r.as_ref().err(), e_r.as_ref().err(), a_r.as_ref().err()]
        .iter()
        .flatten()
    {
        if let Some(crate::error::SjtuCliError::SubSessionStale(name)) = err.downcast_ref() {
            return Err(crate::error::SjtuCliError::SubSessionStale(name).into());
        }
    }

    let schedule = c_r.unwrap_or_else(|e| {
        warnings.push(format!("课表 (N2151) 失败: {e:#}"));
        Schedule::default()
    });
    let exams = e_r.unwrap_or_else(|e| {
        warnings.push(format!("考试 (N358105) 失败: {e:#}"));
        vec![]
    });
    let academic = a_r.unwrap_or_else(|e| {
        warnings.push(format!("学年校历 fixture 失败: {e:#}"));
        AcademicCalendar::default()
    });
    Ok((schedule, exams, academic))
}

/// 三路事件合并：KbItem 课表 / Exam 考试 / AcademicCalendar 校历。
fn unify_events(
    kb_list: &[KbItem],
    exams: &[Exam],
    academic: &AcademicCalendar,
    xnm: &str,
    xqm: &str,
    term_start: chrono::NaiveDate,
) -> Vec<IcsEvent> {
    let mut out = vec![];
    for kb in kb_list {
        if let Some(ev) = from_kb_item(kb, xnm, xqm, term_start) {
            out.push(ev);
        }
    }
    for ex in exams {
        if let Some(ev) = from_exam(ex, xnm, xqm) {
            out.push(ev);
        }
    }
    out.extend(from_academic(academic, xnm, xqm));
    out
}
