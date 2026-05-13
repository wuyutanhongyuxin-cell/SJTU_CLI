//! events.rs 单测（Task 4）：FNV-1a UID + 三路转换。

use super::events::{fnv1a_64, from_academic, from_exam, from_kb_item, term_first_monday, IcsKind};
use crate::apps::jwc::{AcademicCalendar, Exam, Holiday, KbItem};
use chrono::NaiveDate;

#[test]
fn fnv1a_64_standard_testvec() {
    // 标准 FNV-1a 64-bit testvec
    assert_eq!(fnv1a_64("foobar"), "85944171f73967e8");
}

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

#[test]
fn from_kb_item_weekly_course_produces_rrule() {
    let kb = KbItem {
        kcmc: Some("操作系统".into()),
        kch: Some("CS0001".into()),
        xqj: Some("1".into()), // 周一
        jcs: Some("03-04".into()),
        zcd: Some("1-18周".into()),
        cdmc: Some("东上院 102".into()),
        xm: Some("张三".into()),
        ..Default::default()
    };
    let first_monday = NaiveDate::from_ymd_opt(2025, 9, 8).unwrap();
    let ev = from_kb_item(&kb, first_monday).expect("应成功生成 IcsEvent");

    assert_eq!(ev.summary, "操作系统");
    assert_eq!(ev.kind, IcsKind::Schedule);
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
    // Weekly → rrule 存在
    assert!(ev.rrule.is_some(), "Weekly 课应有 rrule");
    assert_eq!(ev.location.as_deref(), Some("东上院 102"));
    assert_eq!(ev.description.as_deref(), Some("教师：张三"));
}

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
    let ev = from_exam(&exam).expect("应成功生成 IcsEvent");

    assert_eq!(ev.summary, "【考试】操作系统");
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
    assert!(ev.rrule.is_none(), "考试不应有 rrule");
}

#[test]
fn from_academic_produces_holiday_events() {
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
    let events = from_academic(&cal);

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].summary, "劳动节");
    assert_eq!(events[0].kind, IcsKind::Holiday);
    assert_eq!(
        events[0].dtstart.format("%Y-%m-%dT%H:%M").to_string(),
        "2026-05-01T00:00"
    );
    // 整天事件 dtend == dtstart（调用方写 VALUE=DATE 格式）
    assert_eq!(events[0].dtstart, events[0].dtend);
    assert_eq!(events[1].summary, "劳动节假期");
}
