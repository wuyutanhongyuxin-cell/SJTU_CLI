//! §8 学年校历获取入口。Fixture 单轨（HTTP endpoint loader 延后）。
//!
//! 查找顺序：
//! 1. `tests/fixtures/jwc/academic_calendar_<xnm>_<xqm>.json`（开发期）
//! 2. `<config_dir>/academic_calendars/<xnm>_<xqm>.json`（部署期，用户自维护）
//! 3. 返 Err（上游 fail-soft）
//!
//! HTTP endpoint loader（cxshjdAreaFive HTML 解析）延后到后续 task —— 目前无
//! HTML 解析依赖（scraper 未加），fixture 路径覆盖 90% 用户场景。

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::apps::jwc::AcademicCalendar;

/// 构造候选路径列表（开发期 fixture → 部署期用户目录）。
fn candidate_paths(xnm: &str, xqm: &str) -> Vec<PathBuf> {
    let filename = format!("academic_calendar_{xnm}_{xqm}.json");
    let user_path = crate::config::config_dir()
        .ok()
        .map(|d| d.join("academic_calendars").join(&filename))
        .unwrap_or_default();
    vec![
        PathBuf::from(format!("tests/fixtures/jwc/{filename}")),
        user_path,
    ]
}

/// 从本地 fixture 文件加载校历。
///
/// 优先读仓库内 `tests/fixtures/jwc/`，其次读
/// `<config_dir>/academic_calendars/<xnm>_<xqm>.json`。
///
/// # Errors
/// 两处路径均不存在时返回 `Err`，消息含候选路径列表。
// TODO(T5-T7): 实装 academic_calendar_from_api(HTML 解析需先加 scraper 依赖)
pub fn load_from_fixture(xnm: &str, xqm: &str) -> Result<AcademicCalendar> {
    let candidates = candidate_paths(xnm, xqm);
    for path in &candidates {
        if path.exists() {
            let raw = std::fs::read_to_string(path)
                .with_context(|| format!("读 {} 失败", path.display()))?;
            let cal: AcademicCalendar = serde_json::from_str(&raw)
                .with_context(|| format!("parse {} 失败", path.display()))?;
            return Ok(cal);
        }
    }
    Err(anyhow::anyhow!(
        "未找到 {xnm}/{xqm} 学期校历 fixture（候选路径: {:?}）",
        candidates
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_2025_12_fixture_from_repo() {
        let cal = load_from_fixture("2025", "12").expect("fixture 已落 T0");
        assert_eq!(cal.xnm.as_deref(), Some("2025"));
        assert_eq!(cal.xqm.as_deref(), Some("12"));
        assert_eq!(cal.xqkssj.as_deref(), Some("2026-02-23"));
        assert_eq!(cal.xqjssj.as_deref(), Some("2026-07-05"));
        // T0 已确认 cxshjdAreaFive endpoint 不返 jjr/tx；fixture 留空 vec
        assert!(cal.jjr.is_empty());
    }

    #[test]
    fn missing_fixture_returns_err() {
        let res = load_from_fixture("9999", "99");
        assert!(res.is_err());
        let msg = format!("{:#}", res.unwrap_err());
        assert!(msg.contains("未找到"));
    }
}
