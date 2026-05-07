//! §2.3 N309131 GPA / 学积分查询响应实体。

use serde::{Deserialize, Serialize};

/// §2.3 N309131 step 2 envelope `items[0]`。
///
/// 通常 `items[]` 长度为 1（当前学生）；多人查询不在 CLI MVP 范围。
///
/// **身份字段全部抹掉**（不进 struct）：`xh / xm / bj / jgmc / zymc / njmc / xh_id`
/// （此 SP 给的 xh_id 是 raw 学号，与 N305005 的 256-hex token 不同，仍按身份处理）。
///
/// **冗余字段不暴露**：`pm1 / pm2`(分项排名内部用) / `tj_id`(临时表 PK，server 内部) /
/// `date / dateDigit / day / month / year` / `jgpxzd / listnav / localeKey /
/// queryModel / userModel / pageable / rangeable / row_id / totalResult`。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Gpa {
    /// GPA。
    #[serde(default)]
    pub gpa: Option<String>,
    /// GPA 排名 `"X/Y"`（自身/总数）。
    #[serde(default)]
    pub gpapm: Option<String>,
    /// 学积分（加权平均分）。
    #[serde(default)]
    pub xjf: Option<String>,
    /// 学积分排名 `"X/Y"`。
    #[serde(default)]
    pub xjfpm: Option<String>,
    /// 总分（成绩加总）。
    #[serde(default)]
    pub zf: Option<String>,
    /// 总门数。
    #[serde(default)]
    pub ms: Option<String>,
    /// 不及格门次（同课多次都计）。
    #[serde(default)]
    pub bjgmc: Option<String>,
    /// 不及格门数（去重）。
    #[serde(default)]
    pub bjgms: Option<String>,
    /// 总学分。
    #[serde(default)]
    pub zxf: Option<String>,
    /// 获得学分。
    #[serde(default)]
    pub hdxf: Option<String>,
    /// 不及格学分。
    #[serde(default)]
    pub bjgxf: Option<String>,
    /// 通过率（"100%" 字符串带百分号）。
    #[serde(default)]
    pub tgl: Option<String>,
    /// 课程范围（回显请求里的 `kcfw`：`hxkc` / `qbkc`）。
    #[serde(default)]
    pub kcfw: Option<String>,
    /// 统计计算时间 `"YYYY-MM-DD HH:MM:SS"`。
    #[serde(default)]
    pub czsj: Option<String>,
}
