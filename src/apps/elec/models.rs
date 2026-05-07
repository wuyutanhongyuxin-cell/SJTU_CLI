//! `elec.sjtu.edu.cn` 响应结构体。契约见 §7.3-7.5 + §7.6。
//!
//! **金额硬约束**：所有金额 / 度数字段用 `rust_decimal::Decimal` + 本文件下方
//! `decimal_str_or_num` 自定义 ser/de，兼容服务端混合类型
//! （`/api/ws/sydl` 是 string，`/api/rechage/*` 是 number）。
//! 序列化输出统一为字符串，避免 JSON 的 f64 精度坑。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// `errno + error + total + entities` 通用 envelope（§7.1）。
///
/// **只反序列化 `entities`**：errno / error / total CLI 都没读
/// （`/api/rechage/ydl` 的 `total:0` 是误标，§7.6，根本不能信），
/// 留着会触发 dead_code。需要时再加。
///
/// **bound 显式重写**：默认 serde derive 从 `#[serde(default)]` + `Vec<T>` 推断
/// `T: Default`，但 `Balance`/`Monthly`/`DailyUsage` 不需要 `Default`。
/// `bound(deserialize = ...)` 把 bound 收紧到只要 `T: Deserialize`。
#[derive(Debug, Clone, Deserialize)]
#[serde(bound(deserialize = "T: serde::Deserialize<'de>"))]
pub(super) struct Envelope<T> {
    #[serde(default)]
    pub entities: Vec<T>,
}

/// `/api/me/info` 当前用户绑定的房间 + 身份。
///
/// **身份字段**（`name` / `work_no`）默认不进入命令层 `BalanceData`，
/// CLI 用户能看到的是 `roomname`（"D10-406"）这类位置标识。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MeInfo {
    #[serde(default)]
    pub account: Option<String>,
    /// 真名（**身份字段**，命令层默认抹掉）。
    #[serde(default)]
    pub name: Option<String>,
    /// 学/工号（**身份字段**）。
    #[serde(default, rename = "workNo")]
    pub work_no: Option<String>,
    #[serde(default)]
    pub dept: Option<String>,
    /// 是否已绑定房间（CLI 检查；未绑定时 balance 等端点会失败）。
    #[serde(default)]
    pub isbind: bool,
    #[serde(default)]
    pub xqdm: Option<String>,
    #[serde(default)]
    pub lddm: Option<String>,
    /// 房间代码（用于日明细 join，`02100406`）。
    #[serde(default)]
    pub roomdm: Option<String>,
    /// 房间名（"D10-406"，UI 友好显示）。
    #[serde(default)]
    pub roomname: Option<String>,
    /// 表号 `mdid`。
    #[serde(default)]
    pub mdid: Option<String>,
}

/// `/api/ws/sydl` 余额。4 个字段服务端发的是字符串（"180.78"）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    /// 剩余度数 (kWh)，`SYL`。
    #[serde(rename = "SYL", with = "decimal_str_or_num")]
    pub remaining_kwh: Decimal,
    /// 剩余补助度数，`SYBZ`。
    #[serde(rename = "SYBZ", with = "decimal_str_or_num")]
    pub subsidy_kwh: Decimal,
    /// 剩余金额（元），`SYLJE`。
    #[serde(rename = "SYLJE", with = "decimal_str_or_num")]
    pub balance_yuan: Decimal,
    /// 剩余补助金额（元），`SYBZJE`。
    #[serde(rename = "SYBZJE", with = "decimal_str_or_num")]
    pub subsidy_balance_yuan: Decimal,
}

/// `/api/rechage/ydl` 月度聚合（注意 `total:0` 误标，§7.6）。
///
/// 字段语义：传 `?year=Y&month=M` 时，服务端返回上月用电（`last`）+ 当月用电（`now`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monthly {
    /// 上月用电度数。
    #[serde(with = "decimal_str_or_num")]
    pub last: Decimal,
    /// 当月用电度数。
    #[serde(with = "decimal_str_or_num")]
    pub now: Decimal,
}

/// `/api/rechage/ydlmx` 日明细。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyUsage {
    #[serde(default)]
    pub roomdm: Option<String>,
    /// 日期 ("YYYY-MM-DD")。
    pub date: String,
    /// 当日用电度数。
    #[serde(with = "decimal_str_or_num")]
    pub used: Decimal,
}

/// 字符串/数字 → `Decimal` 的统一 ser/de。
///
/// **要点**：
/// - serialize：始终输出字符串（避开 JSON f64 精度坑）
/// - deserialize：`deserialize_any`，同时吃 `"180.78"` 和 `80.55`；不支持
///   `deserialize_any` 的格式（bincode 等）会失败 —— 我们只用 JSON。
pub(super) mod decimal_str_or_num {
    use std::fmt;

    use rust_decimal::Decimal;
    use serde::{de, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(d: &Decimal, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&d.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Decimal, D::Error> {
        struct V;
        impl<'de> de::Visitor<'de> for V {
            type Value = Decimal;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a decimal expressed as a string or number")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<Decimal, E> {
                s.parse::<Decimal>().map_err(de::Error::custom)
            }
            fn visit_string<E: de::Error>(self, s: String) -> Result<Decimal, E> {
                self.visit_str(&s)
            }
            fn visit_f64<E: de::Error>(self, n: f64) -> Result<Decimal, E> {
                Decimal::from_str_exact(&n.to_string()).map_err(de::Error::custom)
            }
            fn visit_u64<E: de::Error>(self, n: u64) -> Result<Decimal, E> {
                Ok(Decimal::from(n))
            }
            fn visit_i64<E: de::Error>(self, n: i64) -> Result<Decimal, E> {
                Ok(Decimal::from(n))
            }
        }
        d.deserialize_any(V)
    }
}
