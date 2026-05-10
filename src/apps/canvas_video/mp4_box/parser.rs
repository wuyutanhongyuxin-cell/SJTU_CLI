//! mp4 box parser。所有公开类型见 mod.rs re-export。

use anyhow::{anyhow, bail, Result};

/// 单个 mp4 box header（不含 body）。
// T2/T3 实装 parse_moov 后会真正调用；允许暂时的 dead_code。
#[allow(dead_code)]
pub(super) struct BoxHeader {
    /// box 总长（含 header）。
    pub size: u64,
    /// 4 字节 box type，如 `b"moov"`。
    pub box_type: [u8; 4],
    /// header 字节数（普通 8 / largesize 16）。
    pub header_len: u64,
    /// body 开始的偏移（相对原 buf 起点 = pos + header_len）。
    pub body_start: u64,
}

/// 从 `buf[pos..]` 读 box header。size=1 时取后面 8 字节作 64 位 largesize。
// T2/T3 实装 parse_moov 后会真正调用；允许暂时的 dead_code。
#[allow(dead_code)]
pub(super) fn read_box_header(buf: &[u8], pos: usize) -> Result<BoxHeader> {
    if buf.len() < pos + 8 {
        bail!("box header 截断：pos={pos}, buf.len()={}", buf.len());
    }
    let size32 = u32::from_be_bytes(buf[pos..pos + 4].try_into().unwrap());
    let box_type: [u8; 4] = buf[pos + 4..pos + 8].try_into().unwrap();
    if size32 == 1 {
        if buf.len() < pos + 16 {
            bail!("largesize box header 截断 at {pos}");
        }
        let large = u64::from_be_bytes(buf[pos + 8..pos + 16].try_into().unwrap());
        return Ok(BoxHeader {
            size: large,
            box_type,
            header_len: 16,
            body_start: (pos + 16) as u64,
        });
    }
    if size32 == 0 {
        bail!("size=0（box 延伸到文件尾）暂不支持 at {pos}");
    }
    Ok(BoxHeader {
        size: size32 as u64,
        box_type,
        header_len: 8,
        body_start: (pos + 8) as u64,
    })
}

/// AudioTrack：mux 时所需的全部信息。
#[derive(Debug)]
pub struct AudioTrack {
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u8,
    pub sample_offsets: Vec<u64>,
    pub sample_sizes: Vec<u32>,
    pub mvhd_timescale: u32,
    pub mdhd_timescale: u32,
    pub mdhd_duration: u64,
    pub stsd_raw: Vec<u8>,
}

/// 从 moov box 字节解析 AudioTrack。**入参是 moov 整个 box 的字节**（含 header）。
pub fn parse_moov(_moov_bytes: &[u8]) -> Result<AudioTrack> {
    // 实装见 Task 2 / Task 3
    Err(anyhow!("parse_moov 未实装（Task 2 + Task 3）"))
}
