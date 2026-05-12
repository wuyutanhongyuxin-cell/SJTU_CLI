//! `sjtu jwc <sub>` 命令暴露给 Envelope 的数据形状。
//!
//! 拆分入口；GPA 相关 struct（GpaData / GpaBySemesterData / SemesterGpa / SemesterFailure /
//! SemesterKey）见 `gpa.rs`。

#![allow(dead_code)]

use serde::Serialize;
use serde_json::Value;

use crate::apps::jwc::{Exam, Grade, KbItem, RqAzc};

mod gpa;
pub(in crate::commands::jwc) use gpa::{
    GpaBySemesterData, GpaData, SemesterFailure, SemesterGpa, SemesterKey,
};

/// `sjtu jwc grades` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct GradesData {
    /// 查询入参回显（便于审计）。
    pub xnm: Option<String>,
    pub xqm: Option<String>,
    pub page: u32,
    pub limit: u32,

    /// 服务端 envelope 计数字段（ZF 类型不稳定，原样转出）。
    pub total_result: Option<Value>,
    pub total_page: Option<Value>,

    /// 客户端实际收到的条数（= items.len()）。
    pub returned: usize,

    pub items: Vec<Grade>,
}

/// `sjtu jwc schedule` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct ScheduleData {
    pub xnm: Option<String>,
    pub xqm: Option<String>,
    /// 客户端实际收到的课程条数（= kb_list.len()）。
    pub returned: usize,
    /// 周几文本映射 `{"1":"星期一", ..., "7":"星期日"}`。
    pub xqjmc_map: Value,
    /// 课表条目（已按周几+节次铺平，`zcd` 周次仍是字符串需 parser）。
    pub items: Vec<KbItem>,
}

/// `sjtu jwc exams` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct ExamsData {
    pub xnm: Option<String>,
    pub xqm: Option<String>,
    pub page: u32,
    pub limit: u32,

    pub total_result: Option<Value>,
    pub total_page: Option<Value>,

    pub returned: usize,
    pub items: Vec<Exam>,
}

/// `sjtu jwc today` data 形状。
#[derive(Debug, Serialize)]
pub(super) struct TodayData {
    pub xnm: Option<String>,
    pub xqm: Option<String>,
    pub current_week: u8,
    /// 今日 ISO 日期（"2026-05-12"）。
    pub today_iso: String,
    /// 今日 周几（1=周一 .. 7=周日）。
    pub today_weekday: u8,
    /// 提示文字（如 "学期未开始" / "学期已结束/假期"）；None 时不序列化。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// 已铺平的今日剩余课程（含时刻 / 教室）。
    pub items: Vec<TodayItem>,
}

/// `sjtu jwc week` data 形状。
#[derive(Debug, Serialize)]
pub(super) struct WeekData {
    pub xnm: Option<String>,
    pub xqm: Option<String>,
    pub current_week: u8,
    pub query_zs: u8,
    pub rqazc_list: Vec<RqAzc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    pub items: Vec<WeekItem>,
}

/// `sjtu jwc next` data 形状。
#[derive(Debug, Serialize)]
pub(super) struct NextData {
    pub xnm: Option<String>,
    pub xqm: Option<String>,
    pub current_week: u8,
    pub within_days: u8,
    pub limit: u8,
    pub fetched_weeks: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    pub items: Vec<NextItem>,
}

/// today / week 的单条课程（含时刻 + 节次展开）。
#[derive(Debug, Serialize)]
pub(super) struct TodayItem {
    pub kcmc: Option<String>,
    pub xqj: u8,
    pub jc_list: Vec<u8>,
    /// 与 jc_list 对齐的 (start, end) 时刻字符串（"08:00", "08:45"）。
    pub clock_list: Vec<(String, String)>,
    pub jcor_fallback: Option<String>,
    pub cdmc: Option<String>,
    pub xm: Option<String>,
    pub kch: Option<String>,
}

/// week 的单条课程（同 TodayItem 但语义对应整周；保持独立类型以便后续分歧）。
pub(super) type WeekItem = TodayItem;

/// next 的单条课程（含 absolute datetime）。
#[derive(Debug, Serialize)]
pub(super) struct NextItem {
    pub kcmc: Option<String>,
    pub datetime_start: String,
    pub datetime_end: String,
    pub week: u8,
    pub xqj: u8,
    pub jc_list: Vec<u8>,
    pub cdmc: Option<String>,
    pub xm: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn today_data_serializes_with_all_fields() {
        let d = TodayData {
            xnm: Some("2025".into()),
            xqm: Some("12".into()),
            current_week: 14,
            today_iso: "2026-05-12".into(),
            today_weekday: 2,
            hint: None,
            items: vec![],
        };
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["current_week"], 14);
        assert_eq!(json["today_iso"], "2026-05-12");
    }

    #[test]
    fn week_data_omits_hint_when_none() {
        let d = WeekData {
            xnm: None,
            xqm: None,
            current_week: 14,
            query_zs: 14,
            rqazc_list: vec![],
            hint: None,
            items: vec![],
        };
        let s = serde_json::to_string(&d).unwrap();
        assert!(!s.contains("hint"), "hint=None 应不序列化");
    }

    #[test]
    fn next_data_includes_fetched_weeks_and_limit() {
        let d = NextData {
            xnm: None,
            xqm: None,
            current_week: 14,
            within_days: 7,
            limit: 5,
            fetched_weeks: vec![14, 15],
            hint: None,
            items: vec![],
        };
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["within_days"], 7);
        assert_eq!(json["fetched_weeks"], serde_json::json!([14, 15]));
    }
}
