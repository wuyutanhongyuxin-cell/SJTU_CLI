//! §8 学年校历响应实体。endpoint 与 fixture 共用。
//!
//! 数据来源：`POST /xtgl/index_cxshjdAreaFive.html`（见 tasks/isjtu_investigation.md §8）
//! 该 endpoint 只返学期起止日 + 课表骨架，**不**返节假日 / 调休 —— `jjr` / `tx`
//! 字段保留是为未来如发现新 endpoint 或 fixture 手补能直接复用。

use serde::{Deserialize, Serialize};

/// 学年校历 envelope。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct AcademicCalendar {
    #[serde(default)]
    pub xnm: Option<String>,
    #[serde(default)]
    pub xqm: Option<String>,
    /// 学期开始日期 "YYYY-MM-DD"。
    #[serde(default)]
    pub xqkssj: Option<String>,
    /// 学期结束日期 "YYYY-MM-DD"。
    #[serde(default)]
    pub xqjssj: Option<String>,
    /// 节假日清单（cxshjdAreaFive endpoint 不提供，预留兼容字段）。
    #[serde(default)]
    pub jjr: Vec<Holiday>,
    /// 调休清单（同上）。
    #[serde(default)]
    pub tx: Vec<MakeupClass>,
}

/// 节假日条目。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[allow(dead_code)]
pub struct Holiday {
    /// 日期 "YYYY-MM-DD"。
    #[serde(default)]
    pub rq: Option<String>,
    /// 节假日名称。
    #[serde(default)]
    pub mc: Option<String>,
}

/// 调休条目。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[allow(dead_code)]
pub struct MakeupClass {
    /// 源日期（不上课）。
    #[serde(default)]
    pub src_rq: Option<String>,
    /// 目标日期（上课替代）。
    #[serde(default)]
    pub dst_rq: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture_2025_12() {
        let raw = std::fs::read_to_string("tests/fixtures/jwc/academic_calendar_2025_12.json")
            .expect("fixture 缺失（T0 已落盘）");
        let parsed: AcademicCalendar = serde_json::from_str(&raw).expect("parse");
        assert_eq!(parsed.xnm.as_deref(), Some("2025"));
        assert_eq!(parsed.xqm.as_deref(), Some("12"));
        assert_eq!(parsed.xqkssj.as_deref(), Some("2026-02-23"));
        assert_eq!(parsed.xqjssj.as_deref(), Some("2026-07-05"));
        // T0 已确认 endpoint cxshjdAreaFive 不返 jjr/tx；fixture 是空 vec
        assert!(parsed.jjr.is_empty());
        assert!(parsed.tx.is_empty());
    }

    #[test]
    fn empty_envelope_defaults_to_empty_vecs() {
        let parsed: AcademicCalendar = serde_json::from_str("{}").unwrap();
        assert!(parsed.jjr.is_empty());
        assert!(parsed.tx.is_empty());
        assert!(parsed.xnm.is_none());
    }
}
