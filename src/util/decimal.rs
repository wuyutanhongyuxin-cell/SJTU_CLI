//! 字符串/数字 → `rust_decimal::Decimal` 的统一 ser/de。
//!
//! **要点**：
//! - serialize：始终输出字符串（避开 JSON f64 精度坑）
//! - deserialize：`deserialize_any`，同时吃 `"180.78"` 和 `80.55`；不支持
//!   `deserialize_any` 的格式（bincode 等）会失败 —— 我们只用 JSON。
//!
//! 该 helper 由 `apps/elec/models.rs` 在 S3e 引入，T4 把它从 elec 私有
//! 提到 util 共享，供 `apps/card/` 等子系统并列消费。

use std::fmt;

use rust_decimal::Decimal;
use serde::{de, Deserializer, Serializer};

/// serialize：始终把 `Decimal` 输出为 JSON 字符串（"3.14"）。
pub fn serialize<S: Serializer>(d: &Decimal, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&d.to_string())
}

/// deserialize：兼容服务端混合类型（字符串 `"180.78"` 或数字 `80.55`）。
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

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Wrap {
        #[serde(with = "super")]
        v: Decimal,
    }

    #[test]
    fn de_from_string() {
        let w: Wrap = serde_json::from_str(r#"{"v":"180.78"}"#).unwrap();
        assert_eq!(w.v, Decimal::from_str_exact("180.78").unwrap());
    }

    #[test]
    fn de_from_float() {
        let w: Wrap = serde_json::from_str(r#"{"v":80.55}"#).unwrap();
        assert_eq!(w.v, Decimal::from_str_exact("80.55").unwrap());
    }

    #[test]
    fn de_from_int() {
        let w: Wrap = serde_json::from_str(r#"{"v":100}"#).unwrap();
        assert_eq!(w.v, Decimal::from(100));
    }

    #[test]
    fn de_neg_amount() {
        let w: Wrap = serde_json::from_str(r#"{"v":-10.66}"#).unwrap();
        assert_eq!(w.v, Decimal::from_str_exact("-10.66").unwrap());
    }

    #[test]
    fn ser_always_string() {
        let w = Wrap {
            v: Decimal::from_str_exact("3.14").unwrap(),
        };
        let s = serde_json::to_string(&w).unwrap();
        assert_eq!(s, r#"{"v":"3.14"}"#);
    }
}
