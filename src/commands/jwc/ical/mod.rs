//! `sjtu jwc calendar` 子命令的实现入口：把课表 + 考试 + 学年校历导出为 RFC 5545 .ics。

pub mod dispatch;
pub mod emit;
pub mod events;
pub mod handler;
pub mod recurrence;
pub mod uid;
pub mod vtimezone;
pub mod writer;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_events;
