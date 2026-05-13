//! ical writer / recurrence / events 单测集合。

use super::vtimezone::vtimezone_block;
use super::writer::{emit_line, emit_vevent, fold_line, VEventFields};

#[test]
fn fold_line_does_not_fold_short_lines() {
    let short = "SUMMARY:短课名";
    let folded = fold_line(short);
    assert!(!folded.contains("\r\n"), "短行不应折");
    assert_eq!(folded, short);
}

#[test]
fn fold_line_folds_at_75_octets_for_ascii() {
    let line = "X-CUSTOM:".to_string() + &"a".repeat(200);
    let folded = fold_line(&line);
    let parts: Vec<&str> = folded.split("\r\n").collect();
    // 首行 75 octet，续行每个 74（1 byte SP + 74 content）
    assert!(parts.len() >= 3);
    assert_eq!(parts[0].len(), 75);
    for p in &parts[1..] {
        assert!(p.starts_with(' '), "续行必须以 SP 起");
        assert!(p.len() <= 75);
    }
}

#[test]
fn fold_line_does_not_split_multibyte_chars() {
    // 课程"操作系统原理"+ padding 让边界落在中文字内
    let pad = "A".repeat(70);
    let line = format!("SUMMARY:{}操作系统原理", pad);
    let folded = fold_line(&line);
    // 解 fold 重组，断言中文字串保留完整
    let unfolded = folded.replace("\r\n ", "");
    assert_eq!(unfolded, line);
    // 检查没有断在 UTF-8 中字节
    for part in folded.split("\r\n") {
        if !part.is_empty() {
            assert!(std::str::from_utf8(part.as_bytes()).is_ok());
        }
    }
}

#[test]
fn emit_line_appends_crlf() {
    let mut buf = String::new();
    emit_line(&mut buf, "TEST:x");
    assert_eq!(buf, "TEST:x\r\n");
}

#[test]
fn vtimezone_block_is_well_formed() {
    let b = vtimezone_block();
    assert!(b.starts_with("BEGIN:VTIMEZONE\r\n"));
    assert!(b.contains("TZOFFSETTO:+0800\r\n"));
}

#[test]
fn emit_vevent_includes_required_fields() {
    let mut buf = String::new();
    emit_vevent(
        &mut buf,
        &VEventFields {
            uid: "abc123@sjtu-cli",
            dtstamp_utc: "20260513T024105Z",
            dtstart_local: "20251015T080000",
            dtend_local: "20251015T084500",
            summary: "操作系统",
            description: Some("理论课"),
            location: Some("东上院 102"),
            rrule: Some("FREQ=WEEKLY;COUNT=18"),
        },
    );
    assert!(buf.contains("UID:abc123@sjtu-cli\r\n"));
    assert!(buf.contains("DTSTART;TZID=Asia/Shanghai:20251015T080000\r\n"));
    assert!(buf.contains("SUMMARY:操作系统\r\n"));
    assert!(buf.contains("RRULE:FREQ=WEEKLY;COUNT=18\r\n"));
}

#[test]
fn emit_vevent_escapes_special_chars() {
    let mut buf = String::new();
    emit_vevent(
        &mut buf,
        &VEventFields {
            uid: "x@sjtu-cli",
            dtstamp_utc: "20260513T024105Z",
            dtstart_local: "20251015T080000",
            dtend_local: "20251015T084500",
            summary: "课;有,逗号\\反斜杠",
            description: None,
            location: None,
            rrule: None,
        },
    );
    assert!(buf.contains(r"SUMMARY:课\;有\,逗号\\反斜杠"));
}
