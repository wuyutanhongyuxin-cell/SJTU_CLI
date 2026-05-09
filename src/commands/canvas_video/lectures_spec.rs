//! `--lectures` 选择器解析：把 `"all"` / `"1"` / `"1-5"` / `"1,3,5-7"` 翻成
//! 排序去重后的 1-based `Vec<u32>`，并对照 total（已审讲数）做越界校验。
//!
//! 设计要点：
//! - 1-based 与 `--lecture N` 对齐，0 视作非法
//! - 区间闭闭 `1-5` = [1,2,3,4,5]
//! - "all" 大小写不敏感，单独使用，不与逗号项混排
//! - 越界 / 空 / 反向区间 / 非数字 → `InvalidInput`，附原 spec 与提示

use crate::error::SjtuCliError;
use anyhow::Result;

/// 把 `--lectures` spec 翻成 1-based 升序无重复的讲序号集合。
pub(super) fn parse_lectures_spec(spec: &str, total: u32) -> Result<Vec<u32>> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return Err(SjtuCliError::InvalidInput("--lectures 不能为空".into()).into());
    }
    if trimmed.eq_ignore_ascii_case("all") {
        if total == 0 {
            return Err(
                SjtuCliError::InvalidInput("课程没有已审视频，无法 --lectures all".into()).into(),
            );
        }
        return Ok((1..=total).collect());
    }
    let mut out: Vec<u32> = Vec::new();
    for part in trimmed.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        if let Some((a, b)) = part.split_once('-') {
            let lo = parse_one(a, spec)?;
            let hi = parse_one(b, spec)?;
            if lo > hi {
                return Err(SjtuCliError::InvalidInput(format!(
                    "--lectures 区间反向：'{part}'（来自 '{spec}'）"
                ))
                .into());
            }
            for v in lo..=hi {
                out.push(v);
            }
        } else {
            out.push(parse_one(part, spec)?);
        }
    }
    out.sort_unstable();
    out.dedup();
    if let Some(&max) = out.last() {
        if max > total {
            return Err(SjtuCliError::InvalidInput(format!(
                "--lectures 含第 {max} 讲，超出已审 {total} 讲"
            ))
            .into());
        }
    }
    Ok(out)
}

/// 单个 token：必须是非零正整数。
fn parse_one(s: &str, full: &str) -> Result<u32> {
    let n: u32 = s.trim().parse().map_err(|_| {
        SjtuCliError::InvalidInput(format!("--lectures 含非数字 token '{s}'（来自 '{full}'）"))
    })?;
    if n == 0 {
        return Err(SjtuCliError::InvalidInput(format!(
            "--lectures 不能含 0（1-based，来自 '{full}'）"
        ))
        .into());
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_expands_to_full_range() {
        assert_eq!(parse_lectures_spec("all", 5).unwrap(), vec![1, 2, 3, 4, 5]);
        assert_eq!(parse_lectures_spec("ALL", 3).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn ranges_and_singles_dedup_and_sort() {
        assert_eq!(
            parse_lectures_spec("3,1,5-7,2", 10).unwrap(),
            vec![1, 2, 3, 5, 6, 7]
        );
        assert_eq!(
            parse_lectures_spec("1-3,2-4", 10).unwrap(),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn rejects_zero_reverse_range_oob_nondigit() {
        assert!(parse_lectures_spec("0", 5).is_err());
        assert!(parse_lectures_spec("5-3", 10).is_err());
        assert!(parse_lectures_spec("1-99", 10).is_err());
        assert!(parse_lectures_spec("a,b", 10).is_err());
    }

    #[test]
    fn empty_and_all_zero_total() {
        assert!(parse_lectures_spec("", 5).is_err());
        assert!(parse_lectures_spec("all", 0).is_err());
    }
}
