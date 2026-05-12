//! SJTU 学期默认值推断。
//!
//! T12 真机暴露的真实约束：N2151 / N2154 都**不接受空 xnm/xqm**（返 JSON `null`）。
//! 调用方传 `None` 时按今天日期推 SJTU 学期惯例填充。
//!
//! - 2-7 月: 春季 (xqm=12), xnm = 上一日历年（学年 = 上年9月-本年7月）
//! - 8 月  : 夏季 (xqm=16), xnm = 上一日历年
//! - 9-12 月: 秋季 (xqm=3),  xnm = 当前日历年
//! - 1 月  : 秋季 (xqm=3),  xnm = 上一日历年

use chrono::{Datelike, NaiveDate};

/// 按今天日期推 SJTU 学期 (xnm, xqm)。
pub(crate) fn default_xnm_xqm_by_date(today: NaiveDate) -> (String, String) {
    let year = today.year();
    let month = today.month();
    match month {
        2..=7 => (format!("{}", year - 1), "12".into()),
        8 => (format!("{}", year - 1), "16".into()),
        9..=12 => (format!("{}", year), "3".into()),
        1 => (format!("{}", year - 1), "3".into()),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_2026() {
        let d = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        assert_eq!(default_xnm_xqm_by_date(d), ("2025".into(), "12".into()));
    }

    #[test]
    fn autumn_2025() {
        let d = NaiveDate::from_ymd_opt(2025, 10, 1).unwrap();
        assert_eq!(default_xnm_xqm_by_date(d), ("2025".into(), "3".into()));
    }

    #[test]
    fn january_belongs_to_previous_autumn() {
        let d = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        assert_eq!(default_xnm_xqm_by_date(d), ("2025".into(), "3".into()));
    }

    #[test]
    fn august_is_summer() {
        let d = NaiveDate::from_ymd_opt(2025, 8, 1).unwrap();
        assert_eq!(default_xnm_xqm_by_date(d), ("2024".into(), "16".into()));
    }
}
