//! SJTU 节次 → 起止时刻映射（fallback 硬编码 1-13 节 + 公开 lookup / bitmask helpers）。
//!
//! 节次时刻来源：tasks/isjtu_investigation.md §2.7 末尾（T0 调研结果）。
//! 信源（两个公开页面 1-12 节完全一致）：
//!   - https://jwc.sjtu.edu.cn/info/1041/1110.htm  (上海交通大学教务处)
//!   - https://designschool.sjtu.edu.cn/cultivate/guide/5b1d6e7f194a1ee3d42cb2159297d668/detail/5f472a62b93c48b6b0e0d1c1  (设计学院)
//!
//! 第 13 节单节 19:50-20:35 按 12 节后 10 分钟休息推算（官方仅给 11-13 连上 18:00-20:20）。

use chrono::NaiveTime;

/// SJTU 闵行 / 徐汇主区作息（13 节制）。信源见上方 module doc。
const DEFAULT_TABLE: [(u32, u32, u32, u32); 13] = [
    // (start_h, start_m, end_h, end_m)
    (8, 0, 8, 45),    // 第 1 节
    (8, 55, 9, 40),   // 第 2 节
    (10, 0, 10, 45),  // 第 3 节
    (10, 55, 11, 40), // 第 4 节
    (12, 0, 12, 45),  // 第 5 节
    (12, 55, 13, 40), // 第 6 节
    (14, 0, 14, 45),  // 第 7 节
    (14, 55, 15, 40), // 第 8 节
    (16, 0, 16, 45),  // 第 9 节
    (16, 55, 17, 40), // 第 10 节
    (18, 0, 18, 45),  // 第 11 节
    (18, 55, 19, 40), // 第 12 节
    (19, 50, 20, 35), // 第 13 节（推算）
];

/// 第 `jc` 节（1-13）的起止时刻；越界返回 None。
pub fn lookup(jc: u8) -> Option<(NaiveTime, NaiveTime)> {
    if jc == 0 || jc as usize > DEFAULT_TABLE.len() {
        return None;
    }
    let (sh, sm, eh, em) = DEFAULT_TABLE[jc as usize - 1];
    Some((
        NaiveTime::from_hms_opt(sh, sm, 0)?,
        NaiveTime::from_hms_opt(eh, em, 0)?,
    ))
}

/// `old_zc` bitmask（位 0=第 1 周）是否包含第 `week` 周。
pub fn is_in_week(old_zc: u32, week: u8) -> bool {
    if week == 0 || week > 32 {
        return false;
    }
    (old_zc >> (week - 1)) & 1 == 1
}

/// `old_jc` bitmask（位 0=第 1 节）展开成节次列表。
pub fn jc_positions(old_jc: u32) -> Vec<u8> {
    (0u32..32)
        .filter(|i| (old_jc >> i) & 1 == 1)
        .map(|i| (i + 1) as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_jc_1_returns_8_00_to_8_45() {
        let (s, e) = lookup(1).expect("第 1 节必须存在");
        assert_eq!(s.format("%H:%M").to_string(), "08:00");
        assert_eq!(e.format("%H:%M").to_string(), "08:45");
    }

    #[test]
    fn lookup_jc_5_returns_12_00_to_12_45() {
        let (s, e) = lookup(5).expect("第 5 节必须存在");
        assert_eq!(s.format("%H:%M").to_string(), "12:00");
        assert_eq!(e.format("%H:%M").to_string(), "12:45");
    }

    #[test]
    fn lookup_jc_0_returns_none() {
        assert!(lookup(0).is_none(), "第 0 节非法");
    }

    #[test]
    fn lookup_jc_14_returns_none() {
        assert!(lookup(14).is_none(), "第 14 节超表");
    }

    #[test]
    fn is_in_week_old_zc_524286_returns_true_for_week_2() {
        // 524286 = 0b1111111111111111110，位 1 = 第 2 周
        assert!(is_in_week(524286, 2));
        assert!(is_in_week(524286, 18));
        assert!(!is_in_week(524286, 1), "位 0 = 0，第 1 周不在");
    }

    #[test]
    fn is_in_week_week_0_returns_false() {
        assert!(!is_in_week(0xFFFF, 0), "第 0 周非法");
    }

    #[test]
    fn is_in_week_week_33_returns_false() {
        assert!(!is_in_week(0xFFFFFFFF, 33), "超 32 位非法");
    }

    #[test]
    fn jc_positions_old_jc_12_returns_3_and_4() {
        // 12 = 0b1100，位 2/3 = 第 3/4 节
        assert_eq!(jc_positions(12), vec![3, 4]);
    }

    #[test]
    fn jc_positions_old_jc_0_returns_empty() {
        assert!(jc_positions(0).is_empty());
    }
}
