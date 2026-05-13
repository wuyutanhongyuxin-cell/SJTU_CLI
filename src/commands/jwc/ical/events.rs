//! 三路 IcsEvent 统一转换层：KbItem 课表 / Exam 考试 / AcademicCalendar 校历。
//!
//! - UID：FNV-1a 64-bit 手卷（零依赖；OQ2 决策见 tasks/todo.md）
//! - term_first_monday：从 `xqkssj` 回退到当周周一
//! - fail-soft：字段缺失或 parse 失败均返 None / 空 Vec

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Weekday};

use crate::apps::jwc::{period_clock, AcademicCalendar, Exam, KbItem};
use crate::commands::jwc::ical::recurrence::{parse_zcd, to_rrule, Recurrence};

/// 三路转换后统一 ICS 事件载体，对应单个 VEVENT 核心字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcsEvent {
    pub uid_seed: String,
    pub summary: String,
    pub dtstart: NaiveDateTime, // Asia/Shanghai 本地
    pub dtend: NaiveDateTime,
    pub location: Option<String>,
    pub description: Option<String>,
    pub recurrence: Option<Recurrence>, // Discrete → None，调用方 explode VEVENT
    pub kind: IcsKind,
}

/// 事件来源标记。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IcsKind {
    Class,    // N2151 课表
    Exam,     // N358105 考试
    Academic, // cxshjdAreaFive 校历
}

// ── term_first_monday ────────────────────────────────────────────────────────

/// 从学期开始日计算第 1 周周一（回退到当周周一）。
pub fn term_first_monday(xqkssj: &str) -> Option<NaiveDate> {
    let date = NaiveDate::parse_from_str(xqkssj, "%Y-%m-%d").ok()?;
    let days_from_monday = date.weekday().num_days_from_monday();
    Some(date - Duration::days(days_from_monday as i64))
}

// ── 私有 helpers ─────────────────────────────────────────────────────────────

/// 解析 "03-04" / "1-2" 节次对。
fn parse_jcs(jcs: &str) -> Option<(u8, u8)> {
    let mut parts = jcs.trim().splitn(2, '-');
    let s: u8 = parts.next()?.trim().parse().ok()?;
    let e: u8 = parts.next()?.trim().parse().ok()?;
    (s != 0 && e != 0 && s <= e).then_some((s, e))
}

/// "1".."7" → chrono Weekday。
fn parse_weekday(xqj: &str) -> Option<Weekday> {
    match xqj.trim() {
        "1" => Some(Weekday::Mon),
        "2" => Some(Weekday::Tue),
        "3" => Some(Weekday::Wed),
        "4" => Some(Weekday::Thu),
        "5" => Some(Weekday::Fri),
        "6" => Some(Weekday::Sat),
        "7" => Some(Weekday::Sun),
        _ => None,
    }
}

fn combine(date: NaiveDate, time: NaiveTime) -> NaiveDateTime {
    date.and_time(time)
}

// ── from_kb_item helpers ─────────────────────────────────────────────────────

/// 计算课表条目的 dtstart / dtend 元组。
fn kb_dt_range(kb: &KbItem, first_monday: NaiveDate) -> Option<(NaiveDateTime, NaiveDateTime)> {
    let weekday = parse_weekday(kb.xqj.as_deref()?)?;
    let jcs_str = kb.jcs.as_deref().unwrap_or("");
    let (start_jc, end_jc) = parse_jcs(jcs_str)?;
    let (start_time, _) = period_clock::lookup(start_jc)?;
    let (_, end_time) = period_clock::lookup(end_jc)?;
    let first_occurrence = first_monday + Duration::days(weekday.num_days_from_monday() as i64);
    Some((
        combine(first_occurrence, start_time),
        combine(first_occurrence, end_time),
    ))
}

/// 组装 KbItem 的 uid_seed 字符串（含学期 token，防跨学期碰撞）。
fn kb_uid_seed(kb: &KbItem, xnm: &str, xqm: &str) -> String {
    let kch = kb.kch.as_deref().unwrap_or("?");
    let xqj = kb.xqj.as_deref().unwrap_or("?");
    let jc = kb.jc.as_deref().unwrap_or("?");
    let zcd = kb.zcd.as_deref().unwrap_or("?");
    format!("{xnm}_{xqm}_class_{kch}_xqj{xqj}_jc{jc}_zcd{zcd}")
}

// ── from_kb_item ─────────────────────────────────────────────────────────────

/// KbItem（N2151 课表）→ IcsEvent。`term_start` = `term_first_monday(xqkssj)`。
pub fn from_kb_item(kb: &KbItem, xnm: &str, xqm: &str, term_start: NaiveDate) -> Option<IcsEvent> {
    let summary = kb.kcmc.as_deref()?.to_string();
    let (dtstart, dtend) = kb_dt_range(kb, term_start)?;
    let recurrence_parsed = kb
        .zcd
        .as_deref()
        .map(parse_zcd)
        .unwrap_or(Recurrence::Discrete { weeks: vec![] });
    let has_rrule = to_rrule(&recurrence_parsed).is_some();
    let uid_seed = kb_uid_seed(kb, xnm, xqm);
    let description = kb.xm.as_deref().map(|t| format!("教师：{t}"));

    Some(IcsEvent {
        uid_seed,
        summary,
        dtstart,
        dtend,
        location: kb.cdmc.clone(),
        description,
        recurrence: has_rrule.then_some(recurrence_parsed),
        kind: IcsKind::Class,
    })
}

// ── from_exam ────────────────────────────────────────────────────────────────

/// 解析 `"YYYY-MM-DD(HH:MM-HH:MM)"` 为 (start, end) NaiveDateTime。
fn parse_kssj(kssj: &str) -> Option<(NaiveDateTime, NaiveDateTime)> {
    let (date_part, rest) = kssj.split_once('(')?;
    let time_part = rest.strip_suffix(')')?;
    let (t_start, t_end) = time_part.split_once('-')?;
    let date = NaiveDate::parse_from_str(date_part.trim(), "%Y-%m-%d").ok()?;
    let ts = NaiveTime::parse_from_str(t_start.trim(), "%H:%M").ok()?;
    let te = NaiveTime::parse_from_str(t_end.trim(), "%H:%M").ok()?;
    Some((combine(date, ts), combine(date, te)))
}

/// Exam（N358105 考试）→ IcsEvent。
pub fn from_exam(exam: &Exam, xnm: &str, xqm: &str) -> Option<IcsEvent> {
    let kssj = exam.kssj.as_deref()?;
    let (dtstart, dtend) = parse_kssj(kssj)?;
    let kcmc = exam.kcmc.as_deref().unwrap_or("考试");
    let ksmc = exam.ksmc.as_deref().unwrap_or("");
    let kch = exam.kch.as_deref().unwrap_or("?");
    let date_str = dtstart.format("%Y%m%d").to_string();
    let description = (!ksmc.is_empty()).then(|| ksmc.to_string());

    Some(IcsEvent {
        uid_seed: format!("{xnm}_{xqm}_exam_{kch}_{date_str}"),
        summary: format!("[考] {kcmc}"),
        dtstart,
        dtend,
        location: exam.cdmc.clone(),
        description,
        recurrence: None,
        kind: IcsKind::Exam,
    })
}

// ── from_academic ────────────────────────────────────────────────────────────

/// AcademicCalendar.jjr → Vec<IcsEvent>（整天事件）。
pub fn from_academic(cal: &AcademicCalendar, xnm: &str, xqm: &str) -> Vec<IcsEvent> {
    cal.jjr
        .iter()
        .filter_map(|h| {
            let rq = h.rq.as_deref()?;
            let mc = h.mc.as_deref().unwrap_or("节假日");
            let date = NaiveDate::parse_from_str(rq, "%Y-%m-%d").ok()?;
            let dtstart = date.and_hms_opt(0, 0, 0)?;
            let dtend = date.and_hms_opt(23, 59, 0)?;
            let date_str = date.format("%Y%m%d").to_string();
            Some(IcsEvent {
                uid_seed: format!("{xnm}_{xqm}_holiday_{date_str}"),
                summary: format!("[校历] {mc}"),
                dtstart,
                dtend,
                location: None,
                description: None,
                recurrence: None,
                kind: IcsKind::Academic,
            })
        })
        .collect()
}
