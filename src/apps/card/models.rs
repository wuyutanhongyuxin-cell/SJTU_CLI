//! `api.sjtu.edu.cn/v1/me/card*` 响应结构体。契约见 spec §5.1-5.4。
//!
//! 金额硬约束：`cardBalance` / `transBalance` / `amount` 服务端发 `double`，
//! 反序列化经 `crate::util::decimal` 转为 `Decimal`；序列化输出字符串（避 JSON f64 精度）。
//!
//! 拼写陷阱：`dateTimAccount`（少个 e）—— 仅 orderBy=dateTimeAccount 时返。
//! `#[serde(rename = "dateTimAccount")]` 锁定服务端原字段名。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// `errno + error + total + entities` 通用 envelope（同 elec/services）。
///
/// **bound 显式重写**：默认 derive 会从 `Vec<T>` 推断 `T: Default`，
/// 但 `CardInfo`/`Transaction` 不需要 Default。把 bound 收紧到只要 `T: Deserialize`。
///
/// `errno`/`error` 字段仅做 serde 反序列化（detect_token_expired_in_body 在 body string 层检测），
/// 代码路径不直接读，故加 allow。
#[derive(Debug, Clone, Deserialize)]
#[serde(bound(deserialize = "T: serde::Deserialize<'de>"))]
pub(super) struct Envelope<T> {
    #[allow(dead_code)]
    #[serde(default)]
    pub errno: Option<i32>,
    #[allow(dead_code)]
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub entities: Vec<T>,
}

/// `GET /v1/me/card` 单条 entity（spec §5.1-5.2）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardInfo {
    /// 身份字段（命令层默认抹掉，仅 `--with-identity` 出）
    #[serde(default)]
    pub user: Option<UserInfo>,
    #[serde(rename = "cardNo")]
    pub card_no: String,
    /// 物理卡号 (`cardId`)。永久不透出到命令层（即使 `--with-identity`）。
    #[serde(rename = "cardId", default)]
    pub card_id: Option<String>,
    /// 绑定银行卡号。`--with-identity` 时脱敏前 4 + `****` + 后 4。
    #[serde(rename = "bankNo", default)]
    pub bank_no: Option<String>,
    #[serde(rename = "expireDate", default)]
    pub expire_date: Option<String>,
    /// 主余额（元）
    #[serde(rename = "cardBalance", with = "crate::util::decimal")]
    pub card_balance: Decimal,
    /// 过渡余额（元）
    #[serde(rename = "transBalance", with = "crate::util::decimal")]
    pub trans_balance: Decimal,
    #[serde(default)]
    pub lost: bool,
    #[serde(default)]
    pub frozen: bool,
    #[serde(rename = "faceType", default)]
    pub face_type: Option<String>,
    /// 含"硕士研究生"等身份描述 → `--with-identity` 才出。
    #[serde(rename = "faceSubType", default)]
    pub face_sub_type: Option<String>,
    /// weixin path 独有：挂失状态文本枚举。OAuth2 path 永 None（仍用 `lost: bool`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lost_status: Option<CardLostStatus>,
    /// weixin path 独有：冻结状态文本枚举。OAuth2 path 永 None（仍用 `frozen: bool`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freeze_status: Option<CardFreezeStatus>,
}

/// 卡用户身份（spec §5.2 user.*）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub organize: Option<Organize>,
}

/// 院系组织（spec §5.2 user.organize）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organize {
    #[serde(default)]
    pub name: Option<String>,
}

/// `GET /v1/me/card/transactions` 单条 entity（spec §5.4）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// 消费时间（Unix ms_ts）。命令层会转为 +08:00 DateTime。
    #[serde(rename = "dateTime")]
    pub date_time_ms: i64,
    /// ⚠️ 拼写陷阱：服务端字段名缺 e。仅 orderBy=dateTimeAccount 时返。
    #[serde(rename = "dateTimAccount", default)]
    pub date_tim_account_ms: Option<i64>,
    #[serde(default)]
    pub system: Option<String>,
    #[serde(rename = "merchantNo", default)]
    pub merchant_no: Option<String>,
    #[serde(default)]
    pub merchant: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// 消费为负、充值为正
    #[serde(with = "crate::util::decimal")]
    pub amount: Decimal,
    /// 交易后卡余额
    #[serde(rename = "cardBalance", with = "crate::util::decimal")]
    pub card_balance: Decimal,
}

/// weixin path 独有：挂失状态字符串 → enum。OAuth2 path 用 `lost: bool` 不走此字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardLostStatus {
    Normal,
    Lost,
}

/// weixin path 独有：冻结状态。OAuth2 path 用 `frozen: bool` 不走此字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardFreezeStatus {
    Normal,
    Frozen,
}
