//! 教务系统（i.sjtu.edu.cn / ZFSOFT 正方）客户端。
//!
//! 职责：
//! - CAS 跳转 → i.sjtu.edu.cn 注入 JSESSIONID + keepalive cookie
//! - 封装 ZF 标准 GET-via-POST：`POST <page>?doType=query&gnmkdm=<gnmkdm>`
//!   + form-urlencoded body + `X-Requested-With: XMLHttpRequest`
//! - MVP：§2.1 N305005 学生成绩查询
//!
//! 后续 SP（课表 / GPA / 考试安排 / 详细成绩 / 学分对照 / 周课表 等）逐个补；
//! 共享 `JwcPage<T>` 分页 envelope 和 `post_form_json` helper。
//!
//! 路径契约：tasks/isjtu_investigation.md（§1 通用范式 + §2 各 SP 详细规格）。
//! 调研期 + 实装期合规红线：CLAUDE.md「i.sjtu / 交我办 硬红线」 + 半自动化模式备忘。

mod api;
mod bind;
mod http;
mod models;
#[cfg(test)]
mod tests_parse;
mod throttle;

pub use api::{Client, LoginMeta};
pub use models::{Grade, JwcPage};
