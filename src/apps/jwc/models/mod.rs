//! ZF 教务响应结构体集合。
//!
//! 设计要点（来自 tasks/isjtu_investigation.md 调研期实测的坑）：
//! - **全 String 序列化**：`xf` / `jd` / `xfjd` / `totalResult` 等 ZF 都给字符串
//!   而非数字，CLI 一律 `String`，让上层（人/Agent）自己 Decimal::from_str_exact
//! - **`cj` mixed types**："86" / "P" / "通过" / "良" / 空 全可能 —— 必须 `Option<String>`
//!   绝不强转 f64
//! - **resilience-by-default**：`#[serde(default)]` + `Option<T>` 抗 ZF 字段漂移；
//!   关键 PK（kch / kcmc / cjbdsj 等）保留必填语义但仍 `Option`
//! - **冗余字段不暴露**：`xh_id`(疑似带签名 token，不是 raw 学号) / `bh_id` / `jxb_id`
//!   / `kch_id` / `zyh_id` / `jg_id` / `userModel` / 嵌套 queryModel / 显示侧字段一律 drop
//! - **身份字段默认抹掉**：`xh / xm / xb / bj / njmc / jgmc / zymc` 一律不进 struct，
//!   即便服务端发了也丢弃（半自动 SOP 的硬红线由 struct 层兜底）
//!
//! 子模块：每个 SP 一个文件（mvp 4 个：grade / schedule / gpa / exam）。

mod exam;
mod gpa;
mod grade;
mod schedule;

pub use exam::Exam;
#[allow(unused_imports)]
pub use gpa::{parse_rank, Gpa, RankPair};
pub use grade::Grade;
pub use schedule::{KbItem, RqAzc, Schedule};

use serde::{Deserialize, Serialize};

/// ZF 标准分页 envelope（§1.3）。`T` 为 items 实体类型。
///
/// 所有计数字段类型不稳定（`totalResult` 是字符串，其余可能 int 也可能 string），
/// 用 `serde_json::Value` 兜底；CLI 端原样转出 YAML/JSON 即可，不强转。
///
/// **专属 envelope**（如 N2151 课表）不复用本结构 —— 各 SP 自己定义。
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
