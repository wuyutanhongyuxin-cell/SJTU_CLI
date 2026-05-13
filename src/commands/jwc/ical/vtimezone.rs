//! Asia/Shanghai VTIMEZONE 静态块（中国全年 UTC+8 无 DST）。
//!
//! 内嵌 VTIMEZONE 是 #1 时区兼容关键 —— TZID-only 不嵌在 Google/Apple/Outlook 上
//! 都会导致时区解析失败或回退到客户端本地时区（subagent 研究 2026-05-13 §4）。

/// Asia/Shanghai VTIMEZONE 块（不含换行）。writer 端拼时按 CRLF 加。
///
/// 字面照搬 IANA tzdb 的中国时区简化形态：
/// - DTSTART:19890101T000000（1989 年后中国停 DST）
/// - TZOFFSET +0800 恒定
pub fn vtimezone_block() -> &'static str {
    concat!(
        "BEGIN:VTIMEZONE\r\n",
        "TZID:Asia/Shanghai\r\n",
        "BEGIN:STANDARD\r\n",
        "DTSTART:19890101T000000\r\n",
        "TZOFFSETFROM:+0800\r\n",
        "TZOFFSETTO:+0800\r\n",
        "TZNAME:CST\r\n",
        "END:STANDARD\r\n",
        "END:VTIMEZONE\r\n",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_starts_and_ends_correctly() {
        let b = vtimezone_block();
        assert!(b.starts_with("BEGIN:VTIMEZONE\r\n"));
        assert!(b.ends_with("END:VTIMEZONE\r\n"));
        assert!(b.contains("TZID:Asia/Shanghai\r\n"));
        assert!(b.contains("TZOFFSETTO:+0800\r\n"));
    }

    #[test]
    fn block_uses_crlf_not_lf() {
        let b = vtimezone_block();
        assert!(!b.contains("\n\n"), "不应有连续换行");
        // 数 CRLF：应有 9 个（每行 1 个）
        assert_eq!(b.matches("\r\n").count(), 9);
    }
}
