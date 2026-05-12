//! T9 衍生命令 helper 函数（pure，无 async，便于单测）。
//!
//! 职责：oldzc bitmask 过滤 / within→周数 映射 / jc 展开 / xqj 解析 / datetime 合并。

use chrono::{Datelike, NaiveDate, NaiveDateTime};

use crate::apps::jwc::{period_clock, KbItem};

/// 过滤出在指定 `week` 周内的课表条目（oldzc bitmask 过滤；None = 不过滤）。
pub(crate) fn filter_kb_in_week(kb: &[KbItem], week: u8) -> Vec<&KbItem> {
    kb.iter()
        .filter(|k| match k.old_zc {
            Some(z) => period_clock::is_in_week(z, week),
            None => true,
        })
        .collect()
}

/// 根据 `within` 天数决定需拉取的周数（覆盖跨周边界）。
pub(crate) fn weeks_to_fetch_for_within(within: u8) -> u8 {
    match within {
        0..=1 => 1,
        2..=7 => 2,
        8..=14 => 3,
        15..=21 => 4,
        _ => 5,
    }
}

/// 展开 oldjc bitmask → (节次列表, 时刻列表)。
pub(super) fn expand_jc(old_jc: Option<u32>) -> (Vec<u8>, Vec<(String, String)>) {
    let Some(jc) = old_jc else {
        return (vec![], vec![]);
    };
    let jcs = period_clock::jc_positions(jc);
    let clocks: Vec<(String, String)> = jcs
        .iter()
        .filter_map(|j| {
            let (s, e) = period_clock::lookup(*j)?;
            Some((s.format("%H:%M").to_string(), e.format("%H:%M").to_string()))
        })
        .collect();
    (jcs, clocks)
}

/// 解析 xqj 字符串（"1".."7"）为 u8；非法返回 0。
pub(super) fn parse_xqj(s: Option<&str>) -> u8 {
    s.and_then(|x| x.parse::<u8>().ok())
        .filter(|n| (1..=7).contains(n))
        .unwrap_or(0)
}

/// 合并日期 + "HH:MM" 字符串为 NaiveDateTime。
pub(super) fn combine_dt(d: NaiveDate, hhmm: &str) -> NaiveDateTime {
    let t = chrono::NaiveTime::parse_from_str(hhmm, "%H:%M")
        .unwrap_or_else(|_| chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    d.and_time(t)
}

/// NaiveDate → ISO 周几（周一=1 .. 周日=7）。
pub(super) fn iso_weekday(d: NaiveDate) -> u8 {
    d.weekday().number_from_monday() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::jwc::KbItem;

    #[test]
    fn filter_by_week_drops_courses_not_in_current_week() {
        let kb = vec![
            // 0xFFFF = 所有 16 周都有，第 14 周在其中
            KbItem {
                kcmc: Some("A".into()),
                old_zc: Some(0xFFFF),
                old_jc: Some(0b1100),
                xqj: Some("2".into()),
                ..Default::default()
            },
            // 0b1 = 仅第 1 周，第 14 周不在
            KbItem {
                kcmc: Some("B".into()),
                old_zc: Some(0b1),
                old_jc: Some(0b1100),
                xqj: Some("2".into()),
                ..Default::default()
            },
        ];
        let filtered = filter_kb_in_week(&kb, 14);
        assert!(filtered.iter().any(|k| k.kcmc.as_deref() == Some("A")));
        assert!(!filtered.iter().any(|k| k.kcmc.as_deref() == Some("B")));
    }

    #[test]
    fn weeks_to_fetch_within_1_returns_1() {
        assert_eq!(weeks_to_fetch_for_within(1), 1);
    }

    #[test]
    fn weeks_to_fetch_within_7_returns_2() {
        assert_eq!(weeks_to_fetch_for_within(7), 2);
    }

    #[test]
    fn weeks_to_fetch_within_31_returns_5() {
        assert_eq!(weeks_to_fetch_for_within(31), 5);
    }
}
