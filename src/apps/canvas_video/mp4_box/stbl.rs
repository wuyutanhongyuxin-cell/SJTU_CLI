//! mp4 box parser — stbl / sample table 层。
//!
//! stbl 内有 stsd（codec 与 channels/sample_rate） + stsc（每 chunk sample 数）
//! + stsz（每 sample 字节数） + stco / co64（每 chunk 在文件中的偏移）。
//!
//! 把这些表展开就得到每个 audio sample 的 (offset, size) 列表。

use anyhow::{anyhow, bail, Result};
use tracing::warn;

/// stsz/stsc/stco/co64 entry count 上限。1 小时 AAC ~155000 sample；1M 留 6× 头。
/// 防御 mp4 头部恶意 count=u32::MAX 触发 GB 级分配。
const MAX_TABLE_ENTRIES: usize = 1_000_000;

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

/// stsd: version+flags(4) + entry_count(4) + entries...
/// 第一 entry 头 8 字节 = entry_size + entry_type（如 mp4a / opus）。
/// mp4a entry 结构：8 size+type + 6 reserved + 2 dref_idx + 8 reserved
///   + 2 channels + 2 sample_size + 2 pre_defined + 2 reserved + 4 sample_rate (16.16 fixed)
fn parse_stsd(body: &[u8]) -> Result<(String, u32, u8)> {
    if body.len() < 8 {
        bail!("stsd 截断");
    }
    let entry_count = u32::from_be_bytes(body[4..8].try_into().unwrap());
    if entry_count == 0 {
        bail!("stsd entry_count=0");
    }
    let entry = &body[8..];
    if entry.len() < 36 {
        bail!("stsd entry 截断");
    }
    let codec_raw: [u8; 4] = entry[4..8].try_into().unwrap();
    let codec = String::from_utf8_lossy(&codec_raw).into_owned();
    // channels 字段是 u16，但 AudioTrack.channels 是 u8（实际 SJTU 视频 ≤ 8 channels）。
    // 对极端值做 saturating cast 避免静默截断；> 255 视为 255。
    let raw_channels = u16::from_be_bytes(entry[24..26].try_into().unwrap());
    let channels = raw_channels.min(255) as u8;
    // sample_rate 是 16.16 fixed-point，整数部分在前 2 字节
    let sr_int = u16::from_be_bytes(entry[32..34].try_into().unwrap()) as u32;
    Ok((codec, sr_int, channels))
}

/// stsz: version+flags(4) + sample_size(4) + sample_count(4) + (若 sample_size==0 时 N 个 size 表)
fn parse_stsz(body: &[u8]) -> Result<Vec<u32>> {
    if body.len() < 12 {
        bail!("stsz 截断");
    }
    let sample_size = u32::from_be_bytes(body[4..8].try_into().unwrap());
    let count = u32::from_be_bytes(body[8..12].try_into().unwrap()) as usize;
    if count > MAX_TABLE_ENTRIES {
        bail!("entry_count 异常大: {count} > {MAX_TABLE_ENTRIES}");
    }
    if sample_size != 0 {
        return Ok(vec![sample_size; count]);
    }
    if body.len() < 12 + count.saturating_mul(4) {
        bail!(
            "stsz 表截断: need {} got {}",
            12 + count.saturating_mul(4),
            body.len()
        );
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = 12 + i * 4;
        out.push(u32::from_be_bytes(body[off..off + 4].try_into().unwrap()));
    }
    Ok(out)
}

/// stsc: version+flags(4) + entry_count(4) + N × (first_chunk(4) + samples_per_chunk(4) + sample_desc_idx(4))
fn parse_stsc(body: &[u8]) -> Result<Vec<(u32, u32, u32)>> {
    if body.len() < 8 {
        bail!("stsc 截断");
    }
    let count = u32::from_be_bytes(body[4..8].try_into().unwrap()) as usize;
    if count > MAX_TABLE_ENTRIES {
        bail!("entry_count 异常大: {count} > {MAX_TABLE_ENTRIES}");
    }
    if body.len() < 8 + count.saturating_mul(12) {
        bail!("stsc 表截断");
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = 8 + i * 12;
        out.push((
            u32::from_be_bytes(body[off..off + 4].try_into().unwrap()),
            u32::from_be_bytes(body[off + 4..off + 8].try_into().unwrap()),
            u32::from_be_bytes(body[off + 8..off + 12].try_into().unwrap()),
        ));
    }
    Ok(out)
}

/// stco / co64: version+flags(4) + entry_count(4) + N × (4 或 8 字节 offset)
fn parse_stco(body: &[u8], is_64: bool) -> Result<Vec<u64>> {
    if body.len() < 8 {
        bail!("stco/co64 截断");
    }
    let count = u32::from_be_bytes(body[4..8].try_into().unwrap()) as usize;
    if count > MAX_TABLE_ENTRIES {
        bail!("entry_count 异常大: {count} > {MAX_TABLE_ENTRIES}");
    }
    let entry_size = if is_64 { 8 } else { 4 };
    if body.len() < 8 + count.saturating_mul(entry_size) {
        bail!("stco/co64 表截断");
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = 8 + i * entry_size;
        let v = if is_64 {
            u64::from_be_bytes(body[off..off + 8].try_into().unwrap())
        } else {
            u32::from_be_bytes(body[off..off + 4].try_into().unwrap()) as u64
        };
        out.push(v);
    }
    Ok(out)
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

#[cfg(test)]
pub(crate) fn parse_stbl_for_test(stbl_body: &[u8]) -> Result<AudioTrack> {
    parse_stbl(stbl_body)
}
