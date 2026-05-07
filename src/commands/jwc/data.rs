//! `sjtu jwc <sub>` 命令暴露给 Envelope 的数据形状。
//!
//! 设计：每个 `cmd_*` 一个 Data struct；分页元信息（current_page / total_result 等）
//! 放在顶层而非嵌进 `JwcPage`，方便 Agent 直接 grep。原始 ZF envelope 的
//! `JwcPage<T>` 仅用于解析，items 取出来 flatten 进顶层。

use serde::Serialize;
use serde_json::Value;

use crate::apps::jwc::{Exam, Gpa, Grade, KbItem};

/// `sjtu jwc grades` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct GradesData {
    /// 查询入参回显（便于审计）。
    pub xnm: Option<String>,
    pub xqm: Option<String>,
    pub page: u32,
    pub limit: u32,

    /// 服务端 envelope 计数字段（ZF 类型不稳定，原样转出）。
    pub total_result: Option<Value>,
    pub total_page: Option<Value>,

    /// 客户端实际收到的条数（= items.len()）。
    pub returned: usize,

    pub items: Vec<Grade>,
}

/// `sjtu jwc schedule` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct ScheduleData {
    pub xnm: Option<String>,
    pub xqm: Option<String>,
    /// 客户端实际收到的课程条数（= kb_list.len()）。
    pub returned: usize,
    /// 周几文本映射 `{"1":"星期一", ..., "7":"星期日"}`。
    pub xqjmc_map: Value,
    /// 课表条目（已按周几+节次铺平，`zcd` 周次仍是字符串需 parser）。
    pub items: Vec<KbItem>,
}

/// `sjtu jwc gpa` 的 data 形状。`items[0]` 通常即当前学生。
#[derive(Debug, Serialize)]
pub(super) struct GpaData {
    /// 查询入参回显。
    pub scope: &'static str, // hxkc / qbkc
    pub rank: &'static str, // njzy / nj / bj
    pub qs_xnxq: Option<String>,
    pub zz_xnxq: Option<String>,

    pub total_result: Option<Value>,
    pub returned: usize,
    pub items: Vec<Gpa>,
}

/// `sjtu jwc exams` 的 data 形状。
#[derive(Debug, Serialize)]
pub(super) struct ExamsData {
    pub xnm: Option<String>,
    pub xqm: Option<String>,
    pub page: u32,
    pub limit: u32,

    pub total_result: Option<Value>,
    pub total_page: Option<Value>,

    pub returned: usize,
    pub items: Vec<Exam>,
}
