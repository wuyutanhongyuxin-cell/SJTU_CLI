//! §2.4 N358105 考试信息查询响应实体。

use serde::{Deserialize, Serialize};

/// §2.4 N358105 单条考试 item 暴露给 CLI 的字段。
///
/// **身份字段抹掉**（不进 struct）：`xh / xh_id / xm / xb / bj / njmc / jgmc / zymc`。
///
/// **冗余字段不暴露**：`sksj`(上课时间，与考试无关) / `jxdd`(=上课地点 ≠ 考场) /
/// `cdjc`(空缩写) / `totalresult / row_id / zxbj`。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Exam {
    /// 学年码（"2025"）。
    #[serde(default)]
    pub xnm: Option<String>,
    /// 学年文本（"2025-2026"）。
    #[serde(default)]
    pub xnmc: Option<String>,
    /// 学期码（"3" / "12" / "16"）。
    #[serde(default)]
    pub xqm: Option<String>,
    /// 学期文本。
    #[serde(default)]
    pub xqmmc: Option<String>,
    /// 考试名（`"YYYY-YYYY-N期末考试"` / `"...期中考试"` / `"...免修考"`）。
    #[serde(default)]
    pub ksmc: Option<String>,
    /// 考试时间复合字符串（**`"YYYY-MM-DD(HH:MM-HH:MM)"`**，CLI 端拆 date+开始+结束）。
    #[serde(default)]
    pub kssj: Option<String>,
    /// 课程号。
    #[serde(default)]
    pub kch: Option<String>,
    /// 课程名。
    #[serde(default)]
    pub kcmc: Option<String>,
    /// 教学班名。
    #[serde(default)]
    pub jxbmc: Option<String>,
    /// 教学班组成。
    #[serde(default)]
    pub jxbzc: Option<String>,
    /// 学分（字符串）。
    #[serde(default)]
    pub xf: Option<String>,
    /// 考核方式（"考试" / "考核"）。
    #[serde(default)]
    pub khfs: Option<String>,
    /// 考试方式（"笔试" / "大作业"）。
    #[serde(default)]
    pub ksfs: Option<String>,
    /// **考场**（"上院 412"），≠ `jxdd`（上课教学地点）。
    #[serde(default)]
    pub cdmc: Option<String>,
    /// 考场编号（"SY412"）。
    #[serde(default)]
    pub cdbh: Option<String>,
    /// 考场校区名（"闵行"）。
    #[serde(default)]
    pub cdxqmc: Option<String>,
    /// 开课学院。
    #[serde(default)]
    pub kkxy: Option<String>,
    /// 监考教师，复合 `"<工号>/<姓名>"`（多教师 `;` 分隔）。
    #[serde(default)]
    pub jsxx: Option<String>,
    /// 时间安排编号 `"学校统一-<ksmc>-<kch>"`，可作去重键。
    #[serde(default)]
    pub sjbh: Option<String>,
    /// 是否补考（"是" / "否"）。
    #[serde(default)]
    pub cxbj: Option<String>,
    /// 培养层次（"本科"）。
    #[serde(default)]
    pub pycc: Option<String>,
}
