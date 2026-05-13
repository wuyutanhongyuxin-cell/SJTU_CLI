//! events.rs 单测（Task 4）：FNV-1a UID + 三路转换。

use super::events::{from_academic, from_exam, from_kb_item, term_first_monday, IcsKind};
use super::uid::{fnv1a_64, make_uid};
use crate::apps::jwc::{AcademicCalendar, Exam, Holiday, KbItem};
use chrono::NaiveDate;

// ── fnv1a_64 ─────────────────────────────────────────────────────────────────

#[test]
fn fnv1a_64_standard_testvec() {
    // 标准 FNV-1a 64-bit testvec
    assert_eq!(fnv1a_64("foobar"), "85944171f73967e8");
}

// ── make_uid ──────────────────────────────────────────────────────────────────

#[test]
fn make_uid_appends_domain() {
    let uid = make_uid("test");
    assert!(uid.ends_with("@sjtu-cli"));
    assert_eq!(uid.len(), 16 + 1 + "sjtu-cli".len());
}

#[test]
fn make_uid_is_deterministic() {
    let a = make_uid("xnm_xqm_class_KCH_xqj1_jc1-2_zcd1-18周");
    let b = make_uid("xnm_xqm_class_KCH_xqj1_jc1-2_zcd1-18周");
    assert_eq!(a, b);
}

#[test]
fn make_uid_differs_for_different_seeds() {
    assert_ne!(make_uid("a"), make_uid("b"));
}

// ── term_first_monday ─────────────────────────────────────────────────────────

#[test]
fn term_first_monday_wednesday_backs_to_monday() {
    // 2025-09-10 是周三，应回退到 2025-09-08（周一）
    let result = term_first_monday("2025-09-10");
    assert_eq!(result, Some(NaiveDate::from_ymd_opt(2025, 9, 8).unwrap()));
}

#[test]
fn term_first_monday_already_monday_unchanged() {
    // 2025-09-08 本身就是周一，不变
    let result = term_first_monday("2025-09-08");
    assert_eq!(result, Some(NaiveDate::from_ymd_opt(2025, 9, 8).unwrap()));
}

// ── from_kb_item ──────────────────────────────────────────────────────────────

#[test]
fn from_kb_item_weekly_course_produces_recurrence() {
    let kb = KbItem {
        kcmc: Some("操作系统".into()),
        kch: Some("CS0001".into()),
        xqj: Some("1".into()), // 周一
        jc: Some("3-4".into()),
        jcs: Some("03-04".into()),
        zcd: Some("1-18周".into()),
        cdmc: Some("东上院 102".into()),
        xm: Some("张三".into()),
        ..Default::default()
    };
    let first_monday = NaiveDate::from_ymd_opt(2025, 9, 8).unwrap();
    let ev = from_kb_item(&kb, "2025", "12", first_monday).expect("应成功生成 IcsEvent");

    assert_eq!(ev.summary, "操作系统");
    assert_eq!(ev.kind, IcsKind::Class);
    // 周一第一次上课 = 2025-09-08，第 3 节 10:00
    assert_eq!(
        ev.dtstart.format("%Y-%m-%dT%H:%M").to_string(),
        "2025-09-08T10:00"
    );
    // 第 4 节结束 11:40
    assert_eq!(
        ev.dtend.format("%Y-%m-%dT%H:%M").to_string(),
        "2025-09-08T11:40"
    );
    // Weekly → recurrence 存在
    assert!(ev.recurrence.is_some(), "Weekly 课应有 recurrence");
    assert_eq!(ev.location.as_deref(), Some("东上院 102"));
    assert_eq!(ev.description.as_deref(), Some("教师：张三"));
    // uid_seed 含学期 token
    assert!(ev.uid_seed.contains("2025_12_class_CS0001"));
}

// ── from_exam ─────────────────────────────────────────────────────────────────

#[test]
fn from_exam_parses_kssj_correctly() {
    let exam = Exam {
        kssj: Some("2026-06-15(09:00-11:00)".into()),
        kcmc: Some("操作系统".into()),
        kch: Some("CS0001".into()),
        cdmc: Some("东上院 102".into()),
        ksmc: Some("2025-2026-2 期末考试".into()),
        ..Default::default()
    };
    let ev = from_exam(&exam, "2025", "12").expect("应成功生成 IcsEvent");

    assert_eq!(ev.summary, "[考] 操作系统");
    assert_eq!(ev.kind, IcsKind::Exam);
    assert_eq!(
        ev.dtstart.format("%Y-%m-%dT%H:%M").to_string(),
        "2026-06-15T09:00"
    );
    assert_eq!(
        ev.dtend.format("%Y-%m-%dT%H:%M").to_string(),
        "2026-06-15T11:00"
    );
    assert_eq!(ev.location.as_deref(), Some("东上院 102"));
    assert!(ev.recurrence.is_none(), "考试不应有 recurrence");
    // uid_seed 含学期 token
    assert!(ev.uid_seed.contains("2025_12_exam_CS0001"));
}

// ── from_academic ─────────────────────────────────────────────────────────────

#[test]
fn from_academic_produces_academic_events() {
    let cal = AcademicCalendar {
        xnm: Some("2025".into()),
        xqm: Some("12".into()),
        xqkssj: Some("2026-02-23".into()),
        xqjssj: Some("2026-07-05".into()),
        jjr: vec![
            Holiday {
                rq: Some("2026-05-01".into()),
                mc: Some("劳动节".into()),
            },
            Holiday {
                rq: Some("2026-05-02".into()),
                mc: Some("劳动节假期".into()),
            },
        ],
        tx: vec![],
    };
    let events = from_academic(&cal, "2025", "12");

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].summary, "[校历] 劳动节");
    assert_eq!(events[0].kind, IcsKind::Academic);
    assert_eq!(
        events[0].dtstart.format("%Y-%m-%dT%H:%M").to_string(),
        "2026-05-01T00:00"
    );
    // dtend = 当天 23:59（writer.rs 输出 DATE-TIME；避免 0-duration 整天不显示）
    assert_eq!(
        events[0].dtend.format("%Y-%m-%dT%H:%M").to_string(),
        "2026-05-01T23:59"
    );
    assert!(events[0].dtstart < events[0].dtend);
    assert_eq!(events[1].summary, "[校历] 劳动节假期");
    // uid_seed 含学期 token
    assert!(events[0].uid_seed.contains("2025_12_holiday_20260501"));
}
