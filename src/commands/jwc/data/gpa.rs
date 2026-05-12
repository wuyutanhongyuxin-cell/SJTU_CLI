//! GPA 相关命令的 Envelope data 形状。
//!
//! - `GpaData` ── `sjtu jwc gpa` 单学期/累计查询
//! - `GpaBySemesterData` ── `sjtu jwc gpa-by-semester` 多学期循环
//!
//! 设计：成功学期与失败学期分双数组（fail-soft），agent 拿 `failed.len()`
//! 即知本次有多少学期被跳过（"未到统计时间" / "网络挂" / "items 空"）。

#![allow(dead_code)]

use serde::Serialize;
use serde_json::Value;

use crate::apps::jwc::{Gpa, RankPair};

/// `sjtu jwc gpa` 的 data 形状。`items[0]` 通常即当前学生。
#[derive(Debug, Serialize)]
pub(in crate::commands::jwc) struct GpaData {
    /// 查询入参回显。
    pub scope: &'static str, // hxkc / qbkc
    pub rank: &'static str, // njzy / nj / bj
    pub qs_xnxq: Option<String>,
    pub zz_xnxq: Option<String>,

    pub total_result: Option<Value>,
    pub returned: usize,
    pub items: Vec<Gpa>,
}

/// `sjtu jwc gpa-by-semester` 的 data 形状。
#[derive(Debug, Serialize)]
pub(in crate::commands::jwc) struct GpaBySemesterData {
    pub scope: &'static str,
    pub rank: &'static str,
    pub xnm_from: u32,
    pub xnm_to: u32,
    /// 请求过的全部 (xnm, xqm) 组合（含落空的）。
    pub requested: Vec<SemesterKey>,
    /// 成功拿到 GPA 的学期，含 parsed RankPair。
    pub succeeded: Vec<SemesterGpa>,
    /// 失败学期：原因含 "items 空" / "未到统计时间" / 原始 err 文案。
    pub failed: Vec<SemesterFailure>,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::jwc) struct SemesterKey {
    pub xnm: String,
    pub xqm: String,
}

/// 多学期场景每条成功记录。比单学期 GpaData.items[0] 少 7 个细节字段
/// （zf/bjgmc/bjgms/hdxf/bjgxf/tgl/kcfw），核心 GPA/排名/学分齐全。
#[derive(Debug, Serialize)]
pub(in crate::commands::jwc) struct SemesterGpa {
    pub xnm: String,
    pub xqm: String,
    pub gpa: Option<String>,
    pub gpapm: Option<String>,
    pub gpapm_parsed: Option<RankPair>,
    pub xjf: Option<String>,
    pub xjfpm: Option<String>,
    pub xjfpm_parsed: Option<RankPair>,
    pub ms: Option<String>,
    pub zxf: Option<String>,
    pub czsj: Option<String>,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::jwc) struct SemesterFailure {
    pub xnm: String,
    pub xqm: String,
    pub reason: String,
}

impl From<&Gpa> for SemesterGpa {
    /// 从 Gpa 截选 11 字段构造。要求调用方已调过 g.fill_parsed()。
    fn from(g: &Gpa) -> Self {
        Self {
            xnm: String::new(), // 由调用方填
            xqm: String::new(), // 由调用方填
            gpa: g.gpa.clone(),
            gpapm: g.gpapm.clone(),
            gpapm_parsed: g.gpapm_parsed.clone(),
            xjf: g.xjf.clone(),
            xjfpm: g.xjfpm.clone(),
            xjfpm_parsed: g.xjfpm_parsed.clone(),
            ms: g.ms.clone(),
            zxf: g.zxf.clone(),
            czsj: g.czsj.clone(),
        }
    }
}
