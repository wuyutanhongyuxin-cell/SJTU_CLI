//! 教务系统（i.sjtu.edu.cn / ZFSOFT 正方）客户端。
//!
//! 职责：
//! - CAS 跳转 → i.sjtu.edu.cn 注入 JSESSIONID + keepalive cookie
//! - 封装 ZF 标准 GET-via-POST：`POST <page>?doType=query&gnmkdm=<gnmkdm>`
//!   + form-urlencoded body + `X-Requested-With: XMLHttpRequest`
//! - 已实装 SP（按 §2 顺序）：
//!     - §2.1 N305005 学生成绩查询  → `Client::grades`
//!     - §2.2 N2151  个人课表查询  → `Client::schedule`
//!     - §2.3 N309131 GPA / 学积分（**两阶段**） → `Client::gpa`
//!     - §2.4 N358105 考试信息查询  → `Client::exams`
//!
//! 后续 SP（§2.5..§2.9 详细成绩 / 修业 / 周课表 / 培养计划 / 毕设）逐个补；
//! 共享 `JwcPage<T>` 分页 envelope 和 `post_form_json` helper。
//!
//! 路径契约：tasks/isjtu_investigation.md（§1 通用范式 + §2 各 SP 详细规格）。
//! 调研期 + 实装期合规红线：CLAUDE.md「i.sjtu / 交我办 硬红线」 + 半自动化模式备忘。

mod api;
mod bind;
mod http;
mod models;
pub mod period_clock;
#[cfg(test)]
mod tests_parse;
mod throttle;

pub use api::{Client, GpaRank, GpaScope, LoginMeta};
pub use models::{parse_rank, Exam, Gpa, Grade, JwcPage, KbItem, RankPair, RqAzc, Schedule};
