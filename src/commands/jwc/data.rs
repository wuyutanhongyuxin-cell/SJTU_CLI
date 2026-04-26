//! `sjtu jwc <sub>` 命令暴露给 Envelope 的数据形状。
//!
//! 设计：每个 `cmd_*` 一个 Data struct；分页元信息（current_page / total_result 等）
//! 放在顶层而非嵌进 `JwcPage`，方便 Agent 直接 grep。原始 ZF envelope 的
//! `JwcPage<T>` 仅用于解析，items 取出来 flatten 进顶层。

use serde::Serialize;
use serde_json::Value;

use crate::apps::jwc::Grade;

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
