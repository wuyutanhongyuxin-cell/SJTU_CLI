//! 三路 IcsEvent 统一转换层：KbItem 课表 / Exam 考试 / AcademicCalendar 校历。
//!
//! - UID：FNV-1a 64-bit 手卷（零依赖；OQ2 决策见 tasks/todo.md）
//! - term_first_monday：从 `xqkssj` 回退到当周周一
//! - fail-soft：字段缺失或 parse 失败均返 None / 空 Vec

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Weekday};

use crate::apps::jwc::{period_clock, AcademicCalendar, Exam, KbItem};
use crate::commands::jwc::ical::recurrence::{parse_zcd, to_rrule, Recurrence};

/// FNV-1a 64-bit hash，零依赖。testvec：`fnv1a_64("foobar") == "85944171f73967e8"`
// TODO(T5-T6): cmd_calendar 落地后删 allow
#[allow(dead_code)]
pub fn fnv1a_64(s: &str) -> String {
    const OFFSET: u64 = 14_695_981_039_346_656_037;
    const PRIME: u64 = 1_099_511_628_211;
    let hash = s
        .bytes()
        .fold(OFFSET, |h, b| (h ^ b as u64).wrapping_mul(PRIME));
    format!("{hash:016x}")
}

/// UID = `fnv1a_64(key)@sjtu-cli`，符合 RFC 5545 unique-id。
// TODO(T5-T6): cmd_calendar 落地后删 allow
#[allow(dead_code)]
pub fn make_uid(key: &str) -> String {
    format!("{}@sjtu-cli", fnv1a_64(key))
}

/// 三路转换后统一 ICS 事件载体，对应单个 VEVENT 核心字段。
// TODO(T5-T6): cmd_calendar 落地后删 allow
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcsEvent {
    pub uid: String,
    pub summary: String,
    pub dtstart: NaiveDateTime, // Asia/Shanghai 本地
    pub dtend: NaiveDateTime,
    pub location: Option<String>,
    pub description: Option<String>,
    pub rrule: Option<Recurrence>, // Discrete → None，调用方 explode VEVENT
    pub kind: IcsKind,
}

/// 事件来源标记。
// TODO(T5-T6): cmd_calendar 落地后删 allow
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IcsKind {
    Schedule, // N2151 课表
    Exam,     // N358105 考试
    Holiday,  // cxshjdAreaFive 校历
}

// ── term_first_monday ────────────────────────────────────────────────────────

/// 从学期开始日计算第 1 周周一（回退到当周周一）。
// TODO(T5-T6): cmd_calendar 落地后删 allow
#[allow(dead_code)]
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

// ── from_kb_item ─────────────────────────────────────────────────────────────

/// KbItem（N2151 课表）→ IcsEvent。`first_monday` = `term_first_monday(xqkssj)`。
// TODO(T5-T6): cmd_calendar 落地后删 allow
#[allow(dead_code)]
pub fn from_kb_item(kb: &KbItem, first_monday: NaiveDate) -> Option<IcsEvent> {
    let summary = kb.kcmc.as_deref()?.to_string();
    let xqj_str = kb.xqj.as_deref()?;
    let weekday = parse_weekday(xqj_str)?;
    let jcs = kb.jcs.as_deref().unwrap_or("");
    let (start_jc, end_jc) = parse_jcs(jcs)?;
    let (start_time, _) = period_clock::lookup(start_jc)?;
    let (_, end_time) = period_clock::lookup(end_jc)?;

    let first_occurrence = first_monday + Duration::days(weekday.num_days_from_monday() as i64);
    let dtstart = combine(first_occurrence, start_time);
    let dtend = combine(first_occurrence, end_time);

    let recurrence = kb
        .zcd
        .as_deref()
        .map(parse_zcd)
        .unwrap_or(Recurrence::Discrete { weeks: vec![] });
    let rrule = to_rrule(&recurrence);
    let kch = kb.kch.as_deref().unwrap_or("?");
    let uid_key = format!("kb:{kch}:d{xqj_str}:j{jcs}");
    let description = kb.xm.as_deref().map(|t| format!("教师：{t}"));

    Some(IcsEvent {
        uid: make_uid(&uid_key),
        summary,
        dtstart,
        dtend,
        location: kb.cdmc.clone(),
        description,
        rrule: rrule.map(|_| recurrence),
        kind: IcsKind::Schedule,
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
// TODO(T5-T6): cmd_calendar 落地后删 allow
#[allow(dead_code)]
pub fn from_exam(exam: &Exam) -> Option<IcsEvent> {
    let kssj = exam.kssj.as_deref()?;
    let (dtstart, dtend) = parse_kssj(kssj)?;
    let kcmc = exam.kcmc.as_deref().unwrap_or("考试");
    let ksmc = exam.ksmc.as_deref().unwrap_or("");
    let kch = exam.kch.as_deref().unwrap_or("?");
    let description = (!ksmc.is_empty()).then(|| ksmc.to_string());

    Some(IcsEvent {
        uid: make_uid(&format!("exam:{kch}:{kssj}")),
        summary: format!("【考试】{kcmc}"),
        dtstart,
        dtend,
        location: exam.cdmc.clone(),
        description,
        rrule: None,
        kind: IcsKind::Exam,
    })
}

// ── from_academic ────────────────────────────────────────────────────────────

/// AcademicCalendar.jjr → Vec<IcsEvent>（整天事件，dtstart = dtend = 当天 00:00）。
// TODO(T5-T6): cmd_calendar 落地后删 allow
#[allow(dead_code)]
pub fn from_academic(cal: &AcademicCalendar) -> Vec<IcsEvent> {
    let zero = NaiveTime::from_hms_opt(0, 0, 0).expect("00:00:00 合法");
    cal.jjr
        .iter()
        .filter_map(|h| {
            let rq = h.rq.as_deref()?;
            let mc = h.mc.as_deref().unwrap_or("节假日");
            let date = NaiveDate::parse_from_str(rq, "%Y-%m-%d").ok()?;
            let dt = combine(date, zero);
            Some(IcsEvent {
                uid: make_uid(&format!("holiday:{rq}")),
                summary: mc.to_string(),
                dtstart: dt,
                dtend: dt,
                location: None,
                description: None,
                rrule: None,
                kind: IcsKind::Holiday,
            })
        })
        .collect()
}
