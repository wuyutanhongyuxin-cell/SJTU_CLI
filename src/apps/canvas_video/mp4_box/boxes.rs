//! mp4 box parser — stbl 子表（stsd / stsc / stsz / stco / co64）解析。

use anyhow::{bail, Result};

/// stsz/stsc/stco/co64 entry count 上限。1 小时 AAC ~155000 sample；1M 留 6× 头。
/// 防御 mp4 头部恶意 count=u32::MAX 触发 GB 级分配。
pub(super) const MAX_TABLE_ENTRIES: usize = 1_000_000;

/// stsd: version+flags(4) + entry_count(4) + entries...
/// 第一 entry 头 8 字节 = entry_size + entry_type（如 mp4a / opus）。
/// mp4a entry 结构：8 size+type + 6 reserved + 2 dref_idx + 8 reserved
///   + 2 channels + 2 sample_size + 2 pre_defined + 2 reserved + 4 sample_rate (16.16 fixed)
pub(super) fn parse_stsd(body: &[u8]) -> Result<(String, u32, u8)> {
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
pub(super) fn parse_stsz(body: &[u8]) -> Result<Vec<u32>> {
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
pub(super) fn parse_stsc(body: &[u8]) -> Result<Vec<(u32, u32, u32)>> {
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
pub(super) fn parse_stco(body: &[u8], is_64: bool) -> Result<Vec<u64>> {
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
