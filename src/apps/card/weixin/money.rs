//! 中文金额字符串 → `Decimal`。
//!
//! weixin HTML 字段如 `"3.88 元"` / `"-33.2 元"` / `"20 元"` 没有 OAuth2 JSON 的 `double` 类型。
//! 这里专门一个解析函数收口，避免 `Decimal::from_str` 直接吃带"元"字符串报错。

use anyhow::{anyhow, Result};
use rust_decimal::Decimal;
use std::str::FromStr;

/// 把 `"3.88 元"` / `"-0.8 元"` / `"20"` / `"  -33.2  元  "` 转为 `Decimal`。
/// 失败：纯字符串 / 空 / 多个小数点 → anyhow Err。
pub fn parse_money_zh(s: &str) -> Result<Decimal> {
    let trimmed = s.trim().trim_end_matches('元').trim();
    if trimmed.is_empty() {
        return Err(anyhow!("空金额字符串"));
    }
    Decimal::from_str(trimmed).map_err(|e| anyhow!("解析金额 `{s}` 失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_positive_with_yuan() {
        assert_eq!(
            parse_money_zh("3.88 元").unwrap(),
            Decimal::from_str("3.88").unwrap()
        );
    }

    #[test]
    fn parses_negative_consumption() {
        assert_eq!(
            parse_money_zh("-33.2 元").unwrap(),
            Decimal::from_str("-33.2").unwrap()
        );
        assert_eq!(
            parse_money_zh("-0.8 元").unwrap(),
            Decimal::from_str("-0.8").unwrap()
        );
    }

    #[test]
    fn parses_integer_topup() {
        assert_eq!(parse_money_zh("20 元").unwrap(), Decimal::from(20));
    }

    #[test]
    fn parses_no_unit_suffix() {
        assert_eq!(
            parse_money_zh("100.5").unwrap(),
            Decimal::from_str("100.5").unwrap()
        );
    }

    #[test]
    fn parses_with_excessive_whitespace() {
        assert_eq!(
            parse_money_zh("  3.14  元  ").unwrap(),
            Decimal::from_str("3.14").unwrap()
        );
    }

    #[test]
    fn parses_zero() {
        assert_eq!(parse_money_zh("0 元").unwrap(), Decimal::ZERO);
        assert_eq!(parse_money_zh("0").unwrap(), Decimal::ZERO);
    }

    #[test]
    fn empty_string_errors() {
        assert!(parse_money_zh("").is_err());
        assert!(parse_money_zh("   ").is_err());
        assert!(parse_money_zh("元").is_err());
    }

    #[test]
    fn garbage_text_errors() {
        assert!(parse_money_zh("abc").is_err());
        assert!(parse_money_zh("1.2.3 元").is_err());
    }
}
