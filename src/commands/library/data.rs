//! `sjtu library <sub>` 的数据形状。每个 `cmd_*` 对应一个 `*Data`。
//!
//! 字段全 Option<String>：服务端漂移不破渲染，命令层把 None 显示为 "—"。

use serde::Serialize;

use crate::apps::library::{Fine, HistoryRow, Loan};

/// `sjtu library loans` 的 data。
#[derive(Debug, Serialize)]
pub(super) struct LoansData {
    /// 当前借阅条数。
    pub count: usize,
    /// 借阅明细。
    pub items: Vec<Loan>,
}

/// `sjtu library history` 的 data。
#[derive(Debug, Serialize)]
pub(super) struct HistoryData {
    /// 历史借阅条数。
    pub count: usize,
    /// 历史明细。
    pub items: Vec<HistoryRow>,
}

/// `sjtu library fines` 的 data。
#[derive(Debug, Serialize)]
pub(super) struct FinesData {
    /// 罚款条数。
    pub count: usize,
    /// 待缴纳数（status == "待缴纳"）。
    pub pending_count: usize,
    /// 罚款明细。
    pub items: Vec<Fine>,
}
