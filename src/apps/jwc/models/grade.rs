//! §2.1 N305005 学生成绩查询响应实体。

use serde::{Deserialize, Serialize};

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
