//! `sjtu jwc calendar` 子命令的实现入口：把课表 + 考试 + 学年校历导出为 RFC 5545 .ics。

pub mod events;
pub mod recurrence;
pub mod vtimezone;
pub mod writer;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_events;
