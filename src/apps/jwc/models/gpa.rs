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
    /// 客户端从 `gpapm` 解析的结构化排名（server 不返回，cmd 层 fill_parsed 填）。
    #[serde(default, skip_deserializing)]
    pub gpapm_parsed: Option<RankPair>,
    /// 客户端从 `xjfpm` 解析的结构化排名（server 不返回，cmd 层 fill_parsed 填）。
    #[serde(default, skip_deserializing)]
    pub xjfpm_parsed: Option<RankPair>,
}

impl Gpa {
    /// 从原始 `gpapm` / `xjfpm` 字符串字段解析填充 `*_parsed`。
    /// fail-soft：解析失败的字段保持 `None`，不抛错。
    pub fn fill_parsed(&mut self) {
        self.gpapm_parsed = self.gpapm.as_deref().and_then(parse_rank);
        self.xjfpm_parsed = self.xjfpm.as_deref().and_then(parse_rank);
    }
}

/// 排名字符串 `"X/Y"` 的结构化表达。仅 client 端使用（server 不返回）。
///
/// 设计点：
/// - `rank` / `total` 保留原始整数（不做单位转换）。
/// - `percentile` = rank / total × 100。fail-soft 边界：
///   - `total == 0` → `None`（除零）
///   - `rank > total` → `None`（数据异常，但保留 rank/total 供审计）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RankPair {
    pub rank: u32,
    pub total: u32,
    pub percentile: Option<f64>,
}

/// fail-soft 解析 ZF 排名字符串 `"X/Y"`。
///
/// 容错策略：
/// - 空字符串 / 全空白 → `None`
/// - 无斜杠 / 两侧不是 u32 → `None`
/// - `total == 0` 或 `rank > total` → `Some(RankPair { rank, total, percentile: None })`
pub fn parse_rank(s: &str) -> Option<RankPair> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (l, r) = trimmed.split_once('/')?;
    let rank: u32 = l.trim().parse().ok()?;
    let total: u32 = r.trim().parse().ok()?;
    let percentile = if total == 0 || rank > total {
        None
    } else {
        Some((rank as f64 / total as f64) * 100.0)
    };
    Some(RankPair {
        rank,
        total,
        percentile,
    })
}
