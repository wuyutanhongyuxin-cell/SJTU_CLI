//! Range 合并：将 audio sample 列表合并成稀疏 HTTP Range 请求，gap < threshold 时 inline。

/// 单段 HTTP Range 字节数上限。
///
/// V5.D 真机 smoke（L10）实测：merge 末尾连续 audio chunks 合成 1 个 16.78 MB Range
/// 落在 file tail（offset 899M / total 916M），CDN 30s 内 0 字节流入 → abort。
/// 4 MB 切分让单段下载在 30 s timeout 内 CDN 必有进度，且段数 < 256 不会爆并发。
pub(super) const MAX_RANGE_SIZE: u64 = 4 * 1024 * 1024;

/// audio sample (offset, size) 列表合并成连续 Range（gap < threshold 时 inline 多下）。
/// 返回 Vec<(start_inclusive, end_inclusive)>。
///
/// 合并后对超过 [`MAX_RANGE_SIZE`] 的段进行均匀切分，避免单段被 CDN 拒绝。
///
/// # Preconditions
/// `samples` 必须按 offset 升序。mp4 `stco`/`co64` 表按 ISO 14496-12 规范天然升序，
/// `expand_sample_offsets` 直接产出。乱序输入不 panic 但产出 Range 数错位。
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
            push_split(&mut merged, cur_start, cur_end);
            cur_start = off;
            cur_end = off + size as u64 - 1;
        }
    }
    push_split(&mut merged, cur_start, cur_end);
    merged
}

/// 把 [s, e] 按 [`MAX_RANGE_SIZE`] 切分并 push 到 out（包含两端）。
fn push_split(out: &mut Vec<(u64, u64)>, s: u64, e: u64) {
    let len = e - s + 1;
    if len <= MAX_RANGE_SIZE {
        out.push((s, e));
        return;
    }
    let mut sub_s = s;
    while sub_s <= e {
        let sub_e = (sub_s + MAX_RANGE_SIZE - 1).min(e);
        out.push((sub_s, sub_e));
        if sub_e == e {
            break;
        }
        sub_s = sub_e + 1;
    }
}
