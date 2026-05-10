//! mp4 box parser。所有公开类型见 mod.rs re-export。

use anyhow::{anyhow, bail, Result};

use super::trak::try_parse_audio_trak;

/// 单个 mp4 box header（不含 body）。
pub(super) struct BoxHeader {
    pub size: u64,         // box 总长（含 header）
    pub box_type: [u8; 4], // 4 字节 box type，如 `b"moov"`
    pub header_len: u64,   // header 字节数（普通 8 / largesize 16）
    #[allow(dead_code)] // Task 3 range 计算用
    pub body_start: u64, // body 开始偏移（= pos + header_len）
}

/// 从 `buf[pos..]` 读 box header。size=1 时取后面 8 字节作 64 位 largesize。
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

/// 从 moov box 字节（含 header）解析第一个 AudioTrack。
pub fn parse_moov(moov_bytes: &[u8]) -> Result<AudioTrack> {
    let head = read_box_header(moov_bytes, 0)?;
    if &head.box_type != b"moov" {
        bail!(
            "入参不是 moov box: type={}",
            String::from_utf8_lossy(&head.box_type)
        );
    }
    if (head.size as usize) > moov_bytes.len() {
        bail!(
            "moov box 声称大小 {} 超出缓冲区 {}",
            head.size,
            moov_bytes.len()
        );
    }
    let body = &moov_bytes[head.header_len as usize..head.size as usize];

    let mut mvhd_timescale: u32 = 0;
    let mut audio_track: Option<AudioTrack> = None;
    iter_children(body, |h, child_body| {
        match &h.box_type {
            b"mvhd" => {
                mvhd_timescale = read_mvhd_timescale(child_body)?;
            }
            b"trak" => {
                if let Some(t) = try_parse_audio_trak(child_body)? {
                    if audio_track.is_none() {
                        audio_track = Some(t);
                    }
                }
            }
            _ => {} // 其他 box 忽略
        }
        Ok(())
    })?;
    let mut t = audio_track.ok_or_else(|| anyhow!("moov 内未找到 audio trak"))?;
    t.mvhd_timescale = mvhd_timescale;
    Ok(t)
}

/// 遍历 box body 内的子 box，对每个调 `f(header, child_body)`。
pub(super) fn iter_children(
    body: &[u8],
    mut f: impl FnMut(&BoxHeader, &[u8]) -> Result<()>,
) -> Result<()> {
    let mut pos = 0usize;
    while pos + 8 <= body.len() {
        let h = read_box_header(body, pos)?;
        let end = pos + h.size as usize;
        if end > body.len() {
            bail!("子 box 越界 at {pos}: end={end} body_len={}", body.len());
        }
        let child_body = &body[pos + h.header_len as usize..end];
        f(&h, child_body)?;
        pos = end;
    }
    Ok(())
}

/// mvhd body：version=0 → timescale@12；version=1 → timescale@20。
fn read_mvhd_timescale(body: &[u8]) -> Result<u32> {
    if body.is_empty() {
        bail!("mvhd body 空");
    }
    let version = body[0];
    let off = if version == 0 { 12 } else { 20 };
    if body.len() < off + 4 {
        bail!("mvhd 截断（version={version}, 需 {} 字节）", off + 4);
    }
    Ok(u32::from_be_bytes(body[off..off + 4].try_into().unwrap()))
}
