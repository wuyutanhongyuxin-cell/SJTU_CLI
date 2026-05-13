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
    // "SUMMARY:"(8) + 65×"A"(65) = 73 bytes；下一个字符"操"(CJK, 3 bytes)
    // 若按 byte 切则 73+3=76>75 会在 CJK 首字节前折断，验证算法必须按 char 边界保留。
    let pad = "A".repeat(65);
    let line = format!("SUMMARY:{}操作系统原理", pad);
    let folded = fold_line(&line);
    // 续行必须以 SP + 中文整体起，证明 fold 在 CJK 字符前折断而非中间
    assert!(
        folded.contains("\r\n 操作系统原理"),
        "续行必须以 SP + 中文整体起，证明 fold 在 CJK 字符前折断而非中间"
    );
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
fn fold_line_exactly_75_octets_does_not_fold() {
    let line = "A".repeat(75);
    let folded = fold_line(&line);
    assert!(!folded.contains("\r\n"), "恰好 75 octet 的行不应折");
    assert_eq!(folded, line);
}

#[test]
fn fold_line_empty_string_returns_empty() {
    assert_eq!(fold_line(""), "");
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

// ── recurrence 单测（Task 3）──────────────────────────────────────────────
use super::recurrence::{parse_zcd, to_rrule, Recurrence};

#[test]
fn parse_zcd_full_semester_weekly() {
    // "1-18周" → Weekly { count: 18 }，全学期连续 18 周
    assert_eq!(parse_zcd("1-18周"), Recurrence::Weekly { count: 18 });
}

#[test]
fn parse_zcd_odd_biweekly() {
    // "1-18周(单)" → 奇数周 1,3,5,...,17 共 9 次
    assert_eq!(
        parse_zcd("1-18周(单)"),
        Recurrence::Biweekly {
            count: 9,
            first_week: 1
        }
    );
}

#[test]
fn parse_zcd_even_biweekly() {
    // "2-18周(双)" → 偶数周 2,4,...,18 共 9 次
    assert_eq!(
        parse_zcd("2-18周(双)"),
        Recurrence::Biweekly {
            count: 9,
            first_week: 2
        }
    );
}

#[test]
fn parse_zcd_discrete_list() {
    // "3,5,7,11周" → Discrete { weeks: [3,5,7,11] }
    assert_eq!(
        parse_zcd("3,5,7,11周"),
        Recurrence::Discrete {
            weeks: vec![3, 5, 7, 11]
        }
    );
}

#[test]
fn parse_zcd_short_span_explodes_discrete() {
    // "1-3周" → span=3 ≤ 3，短开 explode 为 [1,2,3]
    assert_eq!(
        parse_zcd("1-3周"),
        Recurrence::Discrete {
            weeks: vec![1, 2, 3]
        }
    );
}

#[test]
fn parse_zcd_invalid_returns_empty_discrete() {
    // 无效字符串 → fail-soft Discrete { weeks: [] }
    assert_eq!(parse_zcd("无效"), Recurrence::Discrete { weeks: vec![] });
}

#[test]
fn to_rrule_weekly_produces_correct_string() {
    let r = Recurrence::Weekly { count: 18 };
    assert_eq!(to_rrule(&r), Some("FREQ=WEEKLY;COUNT=18".to_string()));
}

#[test]
fn to_rrule_biweekly_produces_interval_2() {
    let r = Recurrence::Biweekly {
        count: 9,
        first_week: 1,
    };
    assert_eq!(
        to_rrule(&r),
        Some("FREQ=WEEKLY;INTERVAL=2;COUNT=9".to_string())
    );
}

#[test]
fn to_rrule_discrete_returns_none() {
    // Discrete 不生成 RRULE，由调用方 explode VEVENT
    let r = Recurrence::Discrete { weeks: vec![3, 5] };
    assert_eq!(to_rrule(&r), None);
}

#[test]
fn parse_zcd_single_week_discrete() {
    // I-3：单整数 "5周" 应对齐 plan 契约 → Discrete { weeks: [5] }
    assert_eq!(parse_zcd("5周"), Recurrence::Discrete { weeks: vec![5] });
}

#[test]
fn parse_zcd_span_4_uses_weekly() {
    // S-1：span=4 是从 explode（≤3）切到 Weekly（≥4）的临界点
    assert_eq!(parse_zcd("1-4周"), Recurrence::Weekly { count: 4 });
}

#[test]
fn parse_zcd_reverse_range_fails_soft() {
    // S-2：起始 > 终止 走 fail-soft 而非 panic
    assert_eq!(parse_zcd("10-5周"), Recurrence::Discrete { weeks: vec![] });
}
