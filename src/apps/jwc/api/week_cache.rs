//! N2154 周次反推 + 24h/1h cache（与 jwc 主流程解耦的 pure I/O 层）。
//!
//! cache 文件：`<cache_dir>/jwc_week_cache.json`，由 `crate::config::jwc_week_cache_path` 解析。
//! cache key：显式 (xnm, xqm) → `"{xnm}-{xqm}"`；任一 None → `"__current__"`。
//! TTL：显式 86_400s（24h），`__current__` 3_600s（1h，避免学期切换误判）。

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 根据第 1 周周一日期与今日日期，计算当前是第几教学周。
/// 负差返回 0（学期未开始）。
pub fn compute_current_week(week1_monday: NaiveDate, today: NaiveDate) -> u8 {
    let delta_days = (today - week1_monday).num_days();
    if delta_days < 0 {
        return 0;
    }
    ((delta_days / 7) + 1) as u8
}

/// cache key：显式 xnm+xqm → `"{xnm}-{xqm}"`；否则 `"__current__"`。
pub fn cache_key(xnm: Option<&str>, xqm: Option<&str>) -> String {
    match (xnm, xqm) {
        (Some(x), Some(q)) => format!("{x}-{q}"),
        _ => "__current__".to_string(),
    }
}

/// cache TTL（秒）：显式 24h，`__current__` 1h（避免学期切换误判）。
pub fn cache_ttl_seconds(xnm: Option<&str>, xqm: Option<&str>) -> i64 {
    match (xnm, xqm) {
        (Some(_), Some(_)) => 86_400,
        _ => 3_600,
    }
}

/// jwk_week_cache.json 内容。key = cache_key, value = (week, fetched_at_ISO)。
#[derive(Debug, Default, Serialize, Deserialize)]
struct WeekCache {
    #[serde(default)]
    entries: BTreeMap<String, CacheEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    week: u8,
    /// ISO datetime with timezone（`chrono::Utc::now().to_rfc3339()`）。
    fetched_at: String,
}

/// 读 cache，若 entry 存在且未超 TTL → 返回 week；否则 None。
pub fn read_cache_if_fresh(key: &str, ttl_seconds: i64) -> Option<u8> {
    let path = crate::config::jwc_week_cache_path().ok()?;
    let bytes = std::fs::read(&path).ok()?;
    let cache: WeekCache = serde_json::from_slice(&bytes).ok()?;
    let entry = cache.entries.get(key)?;
    let fetched: DateTime<Utc> = entry.fetched_at.parse().ok()?;
    let age = (Utc::now() - fetched).num_seconds();
    if age >= 0 && age < ttl_seconds {
        Some(entry.week)
    } else {
        None
    }
}

/// 写 cache（best-effort；失败由调用方决定是否吞）。
pub fn write_cache(key: &str, week: u8) -> Result<()> {
    crate::config::ensure_cache_dir()?;
    let path = crate::config::jwc_week_cache_path()?;
    let mut cache: WeekCache = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    cache.entries.insert(
        key.to_string(),
        CacheEntry {
            week,
            fetched_at: Utc::now().to_rfc3339(),
        },
    );
    let bytes = serde_json::to_vec_pretty(&cache)?;
    std::fs::write(&path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_current_week_2025_09_08_to_2026_05_12_returns_36() {
        let week1_monday = NaiveDate::from_ymd_opt(2025, 9, 8).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        // (2026-05-12) - (2025-09-08) = 246 天；246 / 7 = 35.14 → cw = 36
        assert_eq!(compute_current_week(week1_monday, today), 36);
    }

    #[test]
    fn compute_current_week_same_day_returns_1() {
        let d = NaiveDate::from_ymd_opt(2025, 9, 8).unwrap();
        assert_eq!(compute_current_week(d, d), 1);
    }

    #[test]
    fn compute_current_week_before_semester_returns_0() {
        let week1 = NaiveDate::from_ymd_opt(2025, 9, 8).unwrap();
        let pre = NaiveDate::from_ymd_opt(2025, 9, 1).unwrap();
        assert_eq!(compute_current_week(week1, pre), 0);
    }

    #[test]
    fn cache_key_uses_current_when_xnm_xqm_none() {
        assert_eq!(cache_key(None, None), "__current__");
        assert_eq!(cache_key(Some("2025"), None), "__current__");
        assert_eq!(cache_key(None, Some("12")), "__current__");
    }

    #[test]
    fn cache_key_uses_xnm_xqm_when_both_given() {
        assert_eq!(cache_key(Some("2025"), Some("12")), "2025-12");
    }

    #[test]
    fn cache_ttl_1h_for_current_24h_for_explicit() {
        assert_eq!(cache_ttl_seconds(None, None), 3600);
        assert_eq!(cache_ttl_seconds(Some("2025"), Some("12")), 86400);
    }
}
