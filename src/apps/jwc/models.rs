//! ZF 教务响应结构体。
//!
//! 设计要点（来自 tasks/isjtu_investigation.md 调研期实测的 8 大坑）：
//! - **全 String 序列化**：`xf` / `jd` / `xfjd` / `totalResult` 等 ZF 都给字符串
//!   而非数字，CLI 一律 `String`，让上层（人/Agent）自己 Decimal::from_str_exact
//! - **`cj` mixed types**："86" / "P" / "通过" / "良" / 空 全可能 —— 必须 `Option<String>`
//!   绝不强转 f64
//! - **resilience-by-default**：`#[serde(default)]` + `Option<T>` 抗 ZF 字段漂移；
//!   关键 PK（kch / kcmc / cjbdsj）保留必填
//! - **冗余字段不暴露**：`xh_id`(疑似带签名 token，不是 raw 学号) / `bh_id` / `jxb_id`
//!   / `kch_id` / `zyh_id` / `jg_id` / `userModel` / 嵌套 queryModel 等内部主键 / 显示侧字段一律 drop

use serde::{Deserialize, Serialize};

/// ZF 标准分页 envelope（§1.3）。`T` 为 items 实体类型。
///
/// 所有计数字段类型不稳定（`totalResult` 是字符串，其余可能 int 也可能 string），
/// 用 `serde_json::Value` 兜底；CLI 端原样转出 YAML/JSON 即可，不强转。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JwcPage<T> {
    #[serde(default)]
    pub current_page: Option<serde_json::Value>,
    #[serde(default)]
    pub page_no: Option<serde_json::Value>,
    #[serde(default)]
    pub page_size: Option<serde_json::Value>,
    #[serde(default)]
    pub show_count: Option<serde_json::Value>,
    #[serde(default)]
    pub total_count: Option<serde_json::Value>,
    #[serde(default)]
    pub total_page: Option<serde_json::Value>,
    /// **注意**：ZF 经常给字符串 "52" —— 不要强 i64
    #[serde(default)]
    pub total_result: Option<serde_json::Value>,
    #[serde(default)]
    pub items: Vec<T>,
}

/// §2.1 N305005 学生成绩查询单条 item 暴露给 CLI 的字段。
///
/// 字段命名沿用 ZF 原始 `pinyin abbreviation`，便于跟规格表对照；详见
/// tasks/isjtu_investigation.md §2.1 字段表。冗余字段（bh_id / jxb_id / xh_id /
/// kch_id / zyh_id / jg_id / pageTotal / userModel / queryModel / date(*) / row_id /
/// localeKey / listnav / pageable / rangeable）一律不收，避免泄漏内部主键 + 噪音。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Grade {
    /// 学年文本，如 `"2023-2024"`。
    #[serde(default)]
    pub xnmmc: Option<String>,
    /// 学期文本，如 `"1"` / `"2"` / `"3"`。
    #[serde(default)]
    pub xqmmc: Option<String>,
    /// 课程号，如 `"FL1405"`。
    #[serde(default)]
    pub kch: Option<String>,
    /// 课程中文名。
    #[serde(default)]
    pub kcmc: Option<String>,
    /// 课程英文名。
    #[serde(default)]
    pub kcywmc: Option<String>,
    /// 学分（ZF 给字符串，CLI 不强转 Decimal）。
    #[serde(default)]
    pub xf: Option<String>,
    /// 成绩（"86" / "P" / "通过" / "良" / 空）—— **mixed types，绝不 force f64**。
    #[serde(default)]
    pub cj: Option<String>,
    /// 百分制成绩（部分课 cj 是字母时此处为对应百分制）。
    #[serde(default)]
    pub bfzcj: Option<String>,
    /// 绩点。
    #[serde(default)]
    pub jd: Option<String>,
    /// 加权绩点（= xf * jd）。
    #[serde(default)]
    pub xfjd: Option<String>,
    /// 课程性质，如 `"必修"` / `"限选"` / `"通识核心课程"`。
    #[serde(default)]
    pub kcxzmc: Option<String>,
    /// 主辅修标识：`"主修"` / `"辅修"` / `"二专业"` / `"二学位"` / `"非学位"`。
    #[serde(default)]
    pub kcbj: Option<String>,
    /// 考核方式：`"考试"` / `"考核"`。
    #[serde(default)]
    pub khfsmc: Option<String>,
    /// 教师姓名（多教师以 `;` 分隔）。
    #[serde(default)]
    pub jsxm: Option<String>,
    /// 开课部门。
    #[serde(default)]
    pub kkbmmc: Option<String>,
    /// 教学班名，如 `"(2023-2024-1)-FL1405-01"`。
    #[serde(default)]
    pub jxbmc: Option<String>,
    /// 成绩录入时间，如 `"2024-01-15 10:30:00"`。
    #[serde(default)]
    pub cjbdsj: Option<String>,
}
