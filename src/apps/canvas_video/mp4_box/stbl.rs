//! mp4 box parser — stbl / sample table 层。
//!
//! stbl 内有 stsd（codec 与 channels/sample_rate） + stsc（每 chunk sample 数）
//! + stsz（每 sample 字节数） + stco / co64（每 chunk 在文件中的偏移）。
//!
//! 把这些表展开就得到每个 audio sample 的 (offset, size) 列表。

use anyhow::{anyhow, bail, Result};
use tracing::warn;

use super::boxes::{parse_stco, parse_stsc, parse_stsd, parse_stsz};
use super::parser::{iter_children, AudioTrack};

pub(super) fn parse_stbl(stbl_body: &[u8]) -> Result<AudioTrack> {
    let mut stsd_raw: Vec<u8> = Vec::new();
    let mut codec: String = String::new();
    let mut sample_rate: u32 = 0;
    let mut channels: u8 = 0;

    let mut stsz_sizes: Vec<u32> = Vec::new();
    let mut stsc_entries: Vec<(u32, u32, u32)> = Vec::new(); // first_chunk, samples_per_chunk, sample_desc_idx
    let mut chunk_offsets: Vec<u64> = Vec::new();

    iter_children(stbl_body, |h, b| {
        match &h.box_type {
            b"stsd" => {
                stsd_raw = b.to_vec();
                let (c, sr, ch) = parse_stsd(b)?;
                codec = c;
                sample_rate = sr;
                channels = ch;
            }
            b"stsz" => stsz_sizes = parse_stsz(b)?,
            b"stsc" => stsc_entries = parse_stsc(b)?,
            b"stco" => chunk_offsets = parse_stco(b, false)?,
            b"co64" => chunk_offsets = parse_stco(b, true)?,
            _ => {}
        }
        Ok(())
    })?;

    if codec.is_empty() {
        bail!("stbl 缺 stsd codec");
    }
    if stsz_sizes.is_empty() {
        bail!("stbl 缺 stsz / sample 表为空");
    }
    if chunk_offsets.is_empty() {
        bail!("stbl 缺 stco/co64");
    }
    if stsc_entries.is_empty() {
        bail!("stbl 缺 stsc");
    }

    let sample_offsets = expand_sample_offsets(&stsc_entries, &chunk_offsets, &stsz_sizes)?;
    Ok(AudioTrack {
        codec,
        sample_rate,
        channels,
        sample_offsets,
        sample_sizes: stsz_sizes,
        mvhd_timescale: 0, // parse_moov 回填
        mdhd_timescale: 0, // parse_mdia 回填
        mdhd_duration: 0,  // parse_mdia 回填
        stsd_raw,
    })
}

/// stsc + chunk_offsets + sample_sizes → 每个 sample 的绝对偏移。
/// stsc 描述 "第 first_chunk 起每 chunk 含 samples_per_chunk 个 sample"，按 chunk 累加 sample size 算 offset。
///
/// **假设：** stsc 表按 first_chunk 升序（ISO 14496-12 §8.7.4 规范要求）。malformed mp4
/// 若 stsc 乱序可能拿到错的 samples_per_chunk，但 SJTU CDN 的 mp4 由 ffmpeg 生成不会出现。
fn expand_sample_offsets(
    stsc: &[(u32, u32, u32)],
    chunk_offsets: &[u64],
    sample_sizes: &[u32],
) -> Result<Vec<u64>> {
    let mut out: Vec<u64> = Vec::with_capacity(sample_sizes.len());
    let mut sample_idx = 0usize;
    for (i, &chunk_off) in chunk_offsets.iter().enumerate() {
        let chunk_num_1based = (i + 1) as u32;
        // 找当前 chunk 在 stsc 表中的"段"：last entry whose first_chunk <= chunk_num_1based
        let samples_per_chunk = stsc
            .iter()
            .rev()
            .find(|e| e.0 <= chunk_num_1based)
            .map(|e| e.1)
            .ok_or_else(|| anyhow!("stsc 找不到 chunk {chunk_num_1based} 的段"))?;
        let mut cur_off = chunk_off;
        for _ in 0..samples_per_chunk {
            if sample_idx >= sample_sizes.len() {
                warn!(
                    "expand_sample_offsets: stsc 推算 sample 总数超出 stsz({})，在 chunk {} 截断",
                    sample_sizes.len(),
                    i
                );
                return Ok(out);
            }
            out.push(cur_off);
            cur_off += sample_sizes[sample_idx] as u64;
            sample_idx += 1;
        }
    }
    Ok(out)
}
