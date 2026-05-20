//! `weijieyue.lib.sjtu.edu.cn:8080/wechat/sjtuAuth/*` JSON 响应结构。
//!
//! 真机 schema 来源：L0 chrome MCP 反推 + lib_sjtuFine.js / lib_sjtuHistory.js
//! Mustache template 字段访问。L5 真机 CP 时按实际响应回填精确类型。
//!
//! 设计原则：所有字段默认 `Option<String>` + `#[serde(default)]`，
//! 服务端漂移不破解析；CLI 层负责把 None 渲染为 "—"。

use serde::{Deserialize, Serialize};

/// `/sjtuAuth/getSessionId` 响应：`{result: 1, data: "<50 字符 token>"}` 或 `{result: 0}`。
#[derive(Debug, Deserialize)]
pub(super) struct SessionIdResp {
    pub result: i32,
    #[serde(default)]
    pub data: Option<String>,
}

/// `/sjtuAuth/getPidFromSession` 响应（健康检查用）：`{result: 1, data: "<pid>"}`。
#[derive(Debug, Deserialize)]
pub(super) struct PidResp {
    pub result: i32,
    #[serde(default)]
    pub data: Option<String>,
}

/// `/sjtuAuth/getInfo` 响应。**字段名按 L0 推测，L5 真机回填。**
///
/// OQ-LIB-1：实际 borrow 数组字段名未抓到，推测 `borrowArray`；若真机是
/// `nowlendArray` / `currentBorrows` / 别的，serde rename + 回填 fixture。
#[derive(Debug, Deserialize, Default)]
pub(super) struct GetInfoResp {
    pub result: i32,
    #[serde(default, rename = "borrowArray")]
    pub borrow_array: Vec<Loan>,
    /// 是否可续借（全局 flag，影响 `Loan` 渲染）。L0 推测，L5 待回填。
    #[serde(default)]
    #[allow(dead_code)]
    pub can_renew: Option<bool>,
}

/// `/sjtuAuth/getHistoryBorrow` 响应。`historyArray + canRenew` 字段名
/// 在 lib_sjtuHistory.js:9-11 直接出现，可靠。
#[derive(Debug, Deserialize, Default)]
pub(super) struct HistoryBorrowResp {
    pub result: i32,
    #[serde(default, rename = "historyArray")]
    pub history_array: Vec<HistoryRow>,
    /// 是否可续借（history 渲染保留字段）。L0 推测，L5 待回填。
    #[serde(default)]
    #[allow(dead_code)]
    pub can_renew: Option<bool>,
}

/// `/sjtuAuth/getFineInfo` 响应。`fineArray + status` 字段名在
/// lib_sjtuFine.js:7+13 直接出现，可靠。
#[derive(Debug, Deserialize, Default)]
pub(super) struct FineInfoResp {
    pub result: i32,
    #[serde(default, rename = "fineArray")]
    pub fine_array: Vec<Fine>,
}

/// 单条当前借阅。字段名 L5 真机回填精确版。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Loan {
    #[serde(default)]
    pub title: Option<String>,
    /// ISBN（lib_sjtuHistory.js:16 直接取 `item.isbn`）。
    #[serde(default)]
    pub isbn: Option<String>,
    /// 借阅条码 / 馆藏号。L0 推测。
    #[serde(default)]
    pub barcode: Option<String>,
    /// 借阅日期。
    #[serde(default, rename = "borrowDate")]
    pub borrow_date: Option<String>,
    /// 应还日期。
    #[serde(default, rename = "dueDate")]
    pub due_date: Option<String>,
    /// 续借次数。
    #[serde(default, rename = "renewTimes")]
    pub renew_times: Option<i32>,
    /// 馆藏地。
    #[serde(default)]
    pub location: Option<String>,
}

/// 历史借阅一条。字段集 ≈ Loan，多 `returnDate`。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistoryRow {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub isbn: Option<String>,
    #[serde(default)]
    pub barcode: Option<String>,
    #[serde(default, rename = "borrowDate")]
    pub borrow_date: Option<String>,
    #[serde(default, rename = "returnDate")]
    pub return_date: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
}

/// 罚款一条。字段名 L0 已知（lib_sjtuFine.js:13/18/26）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Fine {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub isbn: Option<String>,
    /// 罚款金额（**字符串避免 f64 精度坑**，命令层用 Decimal 解析）。
    #[serde(default, rename = "fineSum")]
    pub fine_sum: Option<String>,
    /// 状态："待缴纳" / "已支付" / "已免除"。
    #[serde(default)]
    pub status: Option<String>,
    /// 罚款日期。
    #[serde(default, rename = "fineDate")]
    pub fine_date: Option<String>,
    /// 缴费流水号（L0 lib_sjtuFine.js:50-51）。
    #[serde(default)]
    pub sequence: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_resp_ok() {
        let s = r#"{"result":1,"data":"J6RExxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxJL7"}"#;
        let r: SessionIdResp = serde_json::from_str(s).unwrap();
        assert_eq!(r.result, 1);
        assert!(r.data.unwrap().starts_with("J6"));
    }

    #[test]
    fn session_id_resp_fail() {
        let s = r#"{"result":0}"#;
        let r: SessionIdResp = serde_json::from_str(s).unwrap();
        assert_eq!(r.result, 0);
        assert!(r.data.is_none());
    }

    #[test]
    fn get_info_resp_empty_borrow_array() {
        let s = r#"{"result":1,"borrowArray":[],"can_renew":true}"#;
        let r: GetInfoResp = serde_json::from_str(s).unwrap();
        assert_eq!(r.result, 1);
        assert!(r.borrow_array.is_empty());
        assert_eq!(r.can_renew, Some(true));
    }

    #[test]
    fn fine_info_resp_with_pending_fine() {
        let s = r#"{"result":1,"fineArray":[{"title":"测试","fineSum":"3.00","status":"待缴纳"}]}"#;
        let r: FineInfoResp = serde_json::from_str(s).unwrap();
        assert_eq!(r.fine_array.len(), 1);
        assert_eq!(r.fine_array[0].status.as_deref(), Some("待缴纳"));
        assert_eq!(r.fine_array[0].fine_sum.as_deref(), Some("3.00"));
    }

    #[test]
    fn loan_tolerates_missing_fields() {
        // 服务端漂移：只发 title，其它字段缺。
        let s = r#"{"title":"测试"}"#;
        let r: Loan = serde_json::from_str(s).unwrap();
        assert_eq!(r.title.as_deref(), Some("测试"));
        assert!(r.isbn.is_none());
    }
}
