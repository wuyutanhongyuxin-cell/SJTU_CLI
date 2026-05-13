//! ZF N2151 KbItem.zcd 周次字符串解析器 + RRULE 生成器。
//!
//! 四类决策：
//! - `Weekly { count }`：全学期连续，如 "1-18周" → FREQ=WEEKLY;COUNT=18
//! - `Biweekly { count, first_week }`：单/双周，如 "1-18周(单)" → INTERVAL=2
//! - `Discrete { weeks }`：离散列表或 ≤3 周短开，调用方负责 explode VEVENT
//!
//! 解析失败时返回 `Discrete { weeks: vec![] }`（fail-soft，不 panic）。

// TODO(T5-T6): cmd_calendar 落地后删除此 allow；届时 parse_zcd/to_rrule/Recurrence 均被 events.rs 调用。
#![allow(dead_code)]

/// 周次重复类型决策结果。
#[derive(Debug, PartialEq, Eq)]
pub enum Recurrence {
    /// 每周连续 `count` 次（全学期常规课）。
    Weekly { count: u32 },
    /// 隔周 `count` 次，第一次在第 `first_week` 周（单/双周）。
    Biweekly { count: u32, first_week: u32 },
    /// 离散周列表，调用方为每周生成独立 VEVENT。
    /// 空列表表示解析失败（fail-soft）。
    Discrete { weeks: Vec<u32> },
}

/// 解析 ZF N2151 `KbItem.zcd` 周次字符串，返回 `Recurrence` 决策。
///
/// # 支持格式
/// - `"1-18周"` → `Weekly { count: 18 }`
/// - `"1-18周(单)"` → `Biweekly { count: 9, first_week: 1 }`
/// - `"2-18周(双)"` → `Biweekly { count: 9, first_week: 2 }`
/// - `"3,5,7,11周"` → `Discrete { weeks: [3,5,7,11] }`
/// - `"1-3周"` → `Discrete { weeks: [1,2,3] }`（span ≤ 3，短开 explode）
/// - 无法解析 → `Discrete { weeks: [] }`（fail-soft）
pub fn parse_zcd(zcd: &str) -> Recurrence {
    let s = zcd.trim();

    // 优先尝试单/双周格式："N-M周(单)" 或 "N-M周(双)"
    if let Some(r) = try_parse_biweekly(s) {
        return r;
    }

    // 尝试连续区间："N-M周"
    if let Some(r) = try_parse_range(s) {
        return r;
    }

    // 尝试离散列表："a,b,c周"
    if let Some(r) = try_parse_discrete(s) {
        return r;
    }

    // 全部失败，fail-soft
    Recurrence::Discrete { weeks: vec![] }
}

/// 尝试解析 "N-M周(单)" / "N-M周(双)" 格式。
fn try_parse_biweekly(s: &str) -> Option<Recurrence> {
    let (body, parity) = if let Some(b) = s.strip_suffix("周(单)") {
        (b, "odd")
    } else if let Some(b) = s.strip_suffix("周(双)") {
        (b, "even")
    } else {
        return None;
    };

    let (start, end) = parse_range_pair(body)?;

    // 计算该奇偶序列在 [start, end] 内的周数
    let count = count_parity_weeks(start, end, parity);
    Some(Recurrence::Biweekly {
        count,
        first_week: start,
    })
}

/// 计算 [start, end] 区间内奇数或偶数周的数量。
fn count_parity_weeks(start: u32, end: u32, parity: &str) -> u32 {
    let is_odd = parity == "odd";
    (start..=end)
        .filter(|&w| if is_odd { w % 2 == 1 } else { w % 2 == 0 })
        .count() as u32
}

/// 尝试解析 "N-M周" 格式（无单双周后缀）。
fn try_parse_range(s: &str) -> Option<Recurrence> {
    let body = s.strip_suffix('周')?;
    // 不含逗号（避免误匹配离散列表）
    if body.contains(',') {
        return None;
    }
    let (start, end) = parse_range_pair(body)?;
    let span = end.saturating_sub(start) + 1;

    if span <= 3 {
        // 短开，explode 成离散列表
        let weeks = (start..=end).collect();
        Some(Recurrence::Discrete { weeks })
    } else {
        Some(Recurrence::Weekly { count: span })
    }
}

/// 将 "N-M" 字符串解析为 `(start, end)` 整数对。
fn parse_range_pair(s: &str) -> Option<(u32, u32)> {
    let mut parts = s.splitn(2, '-');
    let start: u32 = parts.next()?.trim().parse().ok()?;
    let end: u32 = parts.next()?.trim().parse().ok()?;
    if start > end {
        return None;
    }
    Some((start, end))
}

/// 尝试解析 "a,b,c周" 格式。
fn try_parse_discrete(s: &str) -> Option<Recurrence> {
    let body = s.strip_suffix('周')?;
    if !body.contains(',') {
        return None;
    }
    let weeks: Option<Vec<u32>> = body
        .split(',')
        .map(|t| t.trim().parse::<u32>().ok())
        .collect();
    Some(Recurrence::Discrete { weeks: weeks? })
}

/// 将 `Recurrence` 转换为 RFC 5545 RRULE 字符串。
///
/// - `Weekly` / `Biweekly` 返回 `Some(rrule_string)`
/// - `Discrete` 返回 `None`（调用方负责 explode VEVENT，无需 RRULE）
pub fn to_rrule(r: &Recurrence) -> Option<String> {
    match r {
        Recurrence::Weekly { count } => Some(format!("FREQ=WEEKLY;COUNT={count}")),
        Recurrence::Biweekly { count, .. } => Some(format!("FREQ=WEEKLY;INTERVAL=2;COUNT={count}")),
        Recurrence::Discrete { .. } => None,
    }
}
