//! 最小化 m4a 容器写入：ftyp + moov + mdat。
//!
//! 思路：直接借用 source 的 stsd 字节（含 esds AAC 配置），重写 stco 指向新 mdat 起点。
//! mdat 内容是按 sample_offsets 顺序紧密拼接的 audio sample bytes。

use std::path::Path;

use anyhow::{Context, Result};

use crate::apps::canvas_video::mp4_box::AudioTrack;

mod build_moov;
use build_moov::build_moov;

#[cfg(test)]
mod tests;

/// 把 audio_track + sample_bytes 拼成最小 m4a，落到 out 路径。
///
/// `sample_bytes` 必须按 `audio_track.sample_offsets` 顺序拼好，与 `sample_sizes` 一一对应。
/// 返回写入总字节数。
pub fn write_m4a(out: &Path, audio_track: &AudioTrack, sample_bytes: &[u8]) -> Result<u64> {
    let total_samples: u64 = audio_track.sample_sizes.iter().map(|&s| s as u64).sum();
    if sample_bytes.len() as u64 != total_samples {
        anyhow::bail!(
            "sample_bytes 长度 {} != sum(sample_sizes) {}",
            sample_bytes.len(),
            total_samples
        );
    }

    // 先构造 moov（stco 全占位 0），算出 ftyp+moov 总长度后再 rewrite stco。
    let moov_bytes = build_moov(audio_track)?;
    let ftyp = ftyp_box();
    let mdat_header_len: u64 = 8;
    let mdat_offset = ftyp.len() as u64 + moov_bytes.len() as u64 + mdat_header_len;

    let final_moov = rewrite_stco(&moov_bytes, audio_track, mdat_offset)?;
    debug_assert_eq!(final_moov.len(), moov_bytes.len());

    let mut buf = Vec::with_capacity(
        ftyp.len() + final_moov.len() + mdat_header_len as usize + sample_bytes.len(),
    );
    buf.extend_from_slice(&ftyp);
    buf.extend_from_slice(&final_moov);
    // mdat header
    let mdat_size = mdat_header_len + sample_bytes.len() as u64;
    buf.extend_from_slice(&(mdat_size as u32).to_be_bytes());
    buf.extend_from_slice(b"mdat");
    buf.extend_from_slice(sample_bytes);

    let len = buf.len() as u64;
    std::fs::write(out, &buf).with_context(|| format!("写 m4a {}", out.display()))?;
    Ok(len)
}

/// M4A 标准 ftyp box（major=M4A, minor=0x200, compat=[isom, M4A , mp42]）。
fn ftyp_box() -> Vec<u8> {
    let body: &[u8] = b"M4A \x00\x00\x02\x00isomM4A mp42";
    let size = (8 + body.len()) as u32;
    let mut v = Vec::with_capacity(8 + body.len());
    v.extend_from_slice(&size.to_be_bytes());
    v.extend_from_slice(b"ftyp");
    v.extend_from_slice(body);
    v
}

/// 在已构造的 moov 字节里找到 stco，回填每个 chunk_offset（mdat_offset 累加 sample_size）。
fn rewrite_stco(moov: &[u8], track: &AudioTrack, mdat_offset: u64) -> Result<Vec<u8>> {
    let mut out = moov.to_vec();
    let stco_pos = find_box_pos(&out, b"stco")
        .ok_or_else(|| anyhow::anyhow!("build_moov 输出里找不到 stco（内部 bug）"))?;
    let body_start = stco_pos + 8;
    let table_start = body_start + 4 + 4; // version+flags(4) + entry_count(4)
    let mut cur = mdat_offset;
    for (i, &sz) in track.sample_sizes.iter().enumerate() {
        let off = table_start + i * 4;
        if off + 4 > out.len() {
            anyhow::bail!("stco 表写越界（内部 bug）");
        }
        out[off..off + 4].copy_from_slice(&(cur as u32).to_be_bytes());
        cur += sz as u64;
    }
    Ok(out)
}

/// 深度优先在 buf 里查找指定 box type 的起点（仅进入已知容器 box 递归）。
fn find_box_pos(buf: &[u8], box_type: &[u8; 4]) -> Option<usize> {
    let mut pos = 0usize;
    while pos + 8 <= buf.len() {
        let size = u32::from_be_bytes(buf[pos..pos + 4].try_into().ok()?) as usize;
        let ty: [u8; 4] = buf[pos + 4..pos + 8].try_into().ok()?;
        if &ty == box_type {
            return Some(pos);
        }
        if matches!(
            &ty,
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"dinf"
        ) {
            if let Some(inner) = find_box_pos(&buf[pos + 8..pos + size], box_type) {
                return Some(pos + 8 + inner);
            }
        }
        if size == 0 {
            return None;
        }
        pos += size;
    }
    None
}
