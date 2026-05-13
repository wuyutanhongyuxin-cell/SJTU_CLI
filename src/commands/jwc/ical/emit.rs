//! VEVENT 输出 helpers：单事件 / 离散周 explode / 分类计数。
//!
//! 从 handler.rs 拆出，保持 handler.rs 在 200 行以内（宪法限制）。

use chrono::Utc;

use super::events::{IcsEvent, IcsKind};
use super::handler::ByKind;
use super::recurrence::{to_rrule, Recurrence};
use super::uid::{fnv1a_64, make_uid};
use super::writer::{emit_footer, emit_header, emit_vevent, VEventFields};

/// 把 Vec<IcsEvent> 序列化为 RFC 5545 .ics 字节数组；同时统计分类计数。
pub fn emit_ics(events: &[IcsEvent], xnm: &str, xqm: &str) -> (Vec<u8>, ByKind, usize) {
    let calname = format!("SJTU {xnm}-{xqm} 课表 + 考试 + 校历");
    let mut buf = String::new();
    emit_header(&mut buf, &calname);

    let dtstamp_utc = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let mut by_kind = ByKind::default();
    let mut total: usize = 0;

    for ev in events {
        match &ev.recurrence {
            Some(Recurrence::Discrete { weeks }) => {
                // 离散周：explode 为独立 VEVENT，各自偏移天数。
                for &w in weeks {
                    emit_one_explode(&mut buf, ev, w, &dtstamp_utc);
                    total += 1;
                    bump_kind(&mut by_kind, &ev.kind);
                }
            }
            other => {
                // Weekly / Biweekly → RRULE；None（考试/校历）→ 单 VEVENT。
                emit_one(&mut buf, ev, other.as_ref(), &dtstamp_utc);
                total += 1;
                bump_kind(&mut by_kind, &ev.kind);
            }
        }
    }

    emit_footer(&mut buf);
    (buf.into_bytes(), by_kind, total)
}

/// 输出单个 VEVENT（可选 RRULE）。
fn emit_one(buf: &mut String, ev: &IcsEvent, rec: Option<&Recurrence>, dtstamp: &str) {
    let uid = make_uid(&ev.uid_seed);
    let dtstart = ev.dtstart.format("%Y%m%dT%H%M%S").to_string();
    let dtend = ev.dtend.format("%Y%m%dT%H%M%S").to_string();
    let rrule = rec.and_then(to_rrule);
    emit_vevent(
        buf,
        &VEventFields {
            uid: &uid,
            dtstamp_utc: dtstamp,
            dtstart_local: &dtstart,
            dtend_local: &dtend,
            summary: &ev.summary,
            description: ev.description.as_deref(),
            location: ev.location.as_deref(),
            rrule: rrule.as_deref(),
        },
    );
}

/// 离散周 explode：按周偏移 dtstart/dtend，生成独立 VEVENT。
fn emit_one_explode(buf: &mut String, ev: &IcsEvent, week: u32, dtstamp: &str) {
    let off_days = (week as i64 - 1) * 7;
    let dtstart = ev.dtstart + chrono::Duration::days(off_days);
    let dtend = ev.dtend + chrono::Duration::days(off_days);
    let uid = make_uid(&format!("{}_w{week}", ev.uid_seed));
    let ds = dtstart.format("%Y%m%dT%H%M%S").to_string();
    let de = dtend.format("%Y%m%dT%H%M%S").to_string();
    emit_vevent(
        buf,
        &VEventFields {
            uid: &uid,
            dtstamp_utc: dtstamp,
            dtstart_local: &ds,
            dtend_local: &de,
            summary: &ev.summary,
            description: ev.description.as_deref(),
            location: ev.location.as_deref(),
            rrule: None,
        },
    );
}

/// 按 IcsKind 递增对应计数器。
pub fn bump_kind(by: &mut ByKind, kind: &IcsKind) {
    match kind {
        IcsKind::Class => by.class += 1,
        IcsKind::Exam => by.exam += 1,
        IcsKind::Academic => by.academic += 1,
    }
}

/// 计算 .ics 内容的 FNV-1a 16 字符 hex（Envelope 标识，非密码学用途）。
pub fn ics_hash_hex(bytes: &[u8]) -> String {
    fnv1a_64(std::str::from_utf8(bytes).unwrap_or(""))
}
