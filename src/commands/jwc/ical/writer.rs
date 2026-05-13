//! RFC 5545 .ics writer：CRLF 换行 + 75-octet 折行 + VCALENDAR/VEVENT 拼装。
//!
//! 关键硬规则（subagent 研究 2026-05-13）：
//! - CRLF 换行（LF 单独会被 Apple Calendar 静默丢事件）
//! - 75-octet 折行，续行以 SP 开头
//! - 多字节 UTF-8 字符不可在 octet 边界切开（按 char 边界整体保留）
//! - 内嵌 VTIMEZONE Asia/Shanghai
//! - PRODID 必填
//! - X-WR-CALNAME / X-WR-TIMEZONE Google 读其他忽略

use super::vtimezone::vtimezone_block;

/// 按 75 octet 折行；续行以 SP 起；多字节字符整体保留不切。
pub fn fold_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + line.len() / 75);
    let mut current_bytes = 0;
    let max = 75;
    for ch in line.chars() {
        let ch_bytes = ch.len_utf8();
        if current_bytes + ch_bytes > max {
            out.push_str("\r\n ");
            current_bytes = 1; // 续行的 SP 占 1 byte
        }
        out.push(ch);
        current_bytes += ch_bytes;
    }
    out
}

/// 单行 property 输出：fold + CRLF 结尾。
pub fn emit_line(buf: &mut String, line: &str) {
    buf.push_str(&fold_line(line));
    buf.push_str("\r\n");
}

/// VCALENDAR header（包含 VTIMEZONE）。
// TODO(T5-T6): cmd_calendar 落地后删 allow
#[allow(dead_code)]
pub fn emit_header(buf: &mut String, calname: &str) {
    emit_line(buf, "BEGIN:VCALENDAR");
    emit_line(buf, "VERSION:2.0");
    emit_line(buf, "PRODID:-//sjtu-cli//SJTU iCal Export//EN");
    emit_line(buf, "CALSCALE:GREGORIAN");
    emit_line(buf, "METHOD:PUBLISH");
    emit_line(buf, &format!("X-WR-CALNAME:{}", calname));
    emit_line(buf, "X-WR-TIMEZONE:Asia/Shanghai");
    buf.push_str(vtimezone_block());
}

/// VCALENDAR footer。
// TODO(T5-T6): cmd_calendar 落地后删 allow
#[allow(dead_code)]
pub fn emit_footer(buf: &mut String) {
    emit_line(buf, "END:VCALENDAR");
}

/// 把一个 VEVENT 加入 buf。各字段已由 events.rs 准备好为 RFC 5545 string。
///
/// `dtstart_local` / `dtend_local` 格式："20251015T080000"（local time 配 TZID 用）。
// TODO(T5-T6): cmd_calendar 落地后删 allow
#[allow(dead_code)]
pub struct VEventFields<'a> {
    pub uid: &'a str,
    pub dtstamp_utc: &'a str, // "20260513T024105Z"
    pub dtstart_local: &'a str,
    pub dtend_local: &'a str,
    pub summary: &'a str,
    pub description: Option<&'a str>,
    pub location: Option<&'a str>,
    pub rrule: Option<&'a str>,
}

// TODO(T5-T6): cmd_calendar 落地后删 allow
#[allow(dead_code)]
pub fn emit_vevent(buf: &mut String, e: &VEventFields) {
    emit_line(buf, "BEGIN:VEVENT");
    emit_line(buf, &format!("UID:{}", e.uid));
    emit_line(buf, &format!("DTSTAMP:{}", e.dtstamp_utc));
    emit_line(
        buf,
        &format!("DTSTART;TZID=Asia/Shanghai:{}", e.dtstart_local),
    );
    emit_line(buf, &format!("DTEND;TZID=Asia/Shanghai:{}", e.dtend_local));
    emit_line(buf, &format!("SUMMARY:{}", escape_text(e.summary)));
    if let Some(d) = e.description {
        emit_line(buf, &format!("DESCRIPTION:{}", escape_text(d)));
    }
    if let Some(l) = e.location {
        emit_line(buf, &format!("LOCATION:{}", escape_text(l)));
    }
    if let Some(r) = e.rrule {
        emit_line(buf, &format!("RRULE:{}", r));
    }
    emit_line(buf, "END:VEVENT");
}

/// RFC 5545 §3.3.11：TEXT 类型必须转义 `\` / `;` / `,` / 换行。
// 随 emit_vevent 的 #[allow] 一起删除（dead_code 传播链上的从属节点）
#[allow(dead_code)]
fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {} // 吞掉
            _ => out.push(ch),
        }
    }
    out
}
