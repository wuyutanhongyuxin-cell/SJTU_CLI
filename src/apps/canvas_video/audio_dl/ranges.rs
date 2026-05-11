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

/// Dynamic P85 gap_threshold 默认值（fallback）。
///
/// V5.D L10 实测 64 KB 表现尚可（跨 video frame 合并），用 default 兜底
/// 0/1 sample 边界。
const P85_DEFAULT: u64 = 64 * 1024;
/// P85 clamp 下限：< 4 KB 等于不合并（HTTP request 数爆炸）。
const P85_MIN: u64 = 4 * 1024;
/// P85 clamp 上限：> 256 KB 等于合并过激（无效字节占比超 50%）。
const P85_MAX: u64 = 256 * 1024;

/// 计算相邻 audio sample gap 分布的 P85 percentile，作为 merge_ranges 的 gap_threshold。
///
/// V5.E-B+ Dynamic P85 升级（取代 V5.D orchestrator 硬编码 64 KB const）。
///
/// # 算法
/// 1. 对相邻 sample 计算 gap = next_offset - (cur_offset + cur_size)
/// 2. 排序 N-1 个 gap，取 idx = floor(0.85 * (N-1))
/// 3. clamp 到 [P85_MIN, P85_MAX]
///
/// 复杂度 O(N log N)，N = sample_count（V5.D L10 ~55k → 排序 ~ms 级）。
///
/// # 为何 P85（不是 P50/P95）
/// audio sample gap 分布 bimodal + 重右尾（V5.D L10 实测）：
/// - P50 ~10-15 KB（正常 audio-video 交错）
/// - P90 30-60 KB
/// - P99 100-200 KB（I-frame 长尾）
///
/// P50 太低 → 留下大量小 Range；P95 切不掉长尾。P85 是 bimodal 分布的"谷底" —
/// 大多数正常 gap 被合并，I-frame 长尾被切。
///
/// 参考：docs/superpowers/research/2026-05-11-v5e-b-cross-validation.md 发现 3。
///
/// # Preconditions
/// `samples` 必须按 offset 升序（与 [`merge_ranges`] 一致；`stco`/`co64` 表天然满足）。
///
/// # Returns
/// - 0 或 1 sample → [`P85_DEFAULT`]（无 gap 可算）
/// - ≥ 2 sample → P85 gap clamp 到 [[`P85_MIN`], [`P85_MAX`]]
pub(super) fn compute_p85_gap(samples: &[(u64, u32)]) -> u64 {
    if samples.len() < 2 {
        return P85_DEFAULT;
    }
    let mut gaps: Vec<u64> = Vec::with_capacity(samples.len() - 1);
    for w in samples.windows(2) {
        let cur_end = w[0].0 + w[0].1 as u64;
        // saturating_sub 兜底：正常 mp4 gap ≥ 0，畸形文件可能负差不 panic
        let gap = w[1].0.saturating_sub(cur_end);
        gaps.push(gap);
    }
    gaps.sort_unstable();
    let idx = ((gaps.len() as f64 * 0.85).floor() as usize).min(gaps.len() - 1);
    gaps[idx].clamp(P85_MIN, P85_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p85_empty_returns_default() {
        assert_eq!(compute_p85_gap(&[]), 64 * 1024);
    }

    #[test]
    fn p85_single_sample_returns_default() {
        assert_eq!(compute_p85_gap(&[(0, 100)]), 64 * 1024);
    }

    #[test]
    fn p85_bimodal_picks_between_p50_and_p99() {
        // 10 small gap (10 KB) + 90 large gap (100 KB) — P85 应落 large gap 区
        let mut samples = vec![(0u64, 1000u32)];
        let mut next = 1000u64;
        for _ in 0..10 {
            next += 10 * 1024;
            samples.push((next, 1000));
            next += 1000;
        }
        for _ in 0..90 {
            next += 100 * 1024;
            samples.push((next, 1000));
            next += 1000;
        }
        let p85 = compute_p85_gap(&samples);
        assert!(
            p85 >= 50 * 1024,
            "p85 应 ≥ 50 KB（落 large gap 区），实得 {p85}"
        );
        assert!(p85 <= 256 * 1024, "p85 应 ≤ clamp max 256 KB，实得 {p85}");
    }

    #[test]
    fn p85_clamps_to_min_4kb() {
        // 全 1 KB gap → P85 = 1 KB 被 clamp 到 min 4 KB
        let mut samples = vec![(0u64, 100u32)];
        let mut next = 100u64;
        for _ in 0..50 {
            next += 1024;
            samples.push((next, 100));
            next += 100;
        }
        assert!(
            compute_p85_gap(&samples) >= 4 * 1024,
            "应 clamp 到 min 4 KB"
        );
    }
}
