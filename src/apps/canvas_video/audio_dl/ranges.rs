//! Range 合并：将 audio sample 列表合并成稀疏 HTTP Range 请求，gap < threshold 时 inline。

#![allow(dead_code)] // T8 接入后删

/// audio sample (offset, size) 列表合并成连续 Range（gap < threshold 时 inline 多下）。
/// 返回 Vec<(start_inclusive, end_inclusive)>。
pub(super) fn merge_ranges(samples: &[(u64, u32)], gap_threshold: u64) -> Vec<(u64, u64)> {
    if samples.is_empty() {
        return Vec::new();
    }
    let mut merged: Vec<(u64, u64)> = Vec::new();
    let mut cur_start = samples[0].0;
    let mut cur_end = samples[0].0 + samples[0].1 as u64 - 1;
    for &(off, size) in &samples[1..] {
        let gap = off.saturating_sub(cur_end + 1);
        if gap <= gap_threshold {
            cur_end = (off + size as u64 - 1).max(cur_end);
        } else {
            merged.push((cur_start, cur_end));
            cur_start = off;
            cur_end = off + size as u64 - 1;
        }
    }
    merged.push((cur_start, cur_end));
    merged
}
