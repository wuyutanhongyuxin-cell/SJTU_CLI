//! mp4 box parser — trak / mdia 层。从 trak 找到 audio mdia，递归到 stbl。

use anyhow::{anyhow, Result};

use super::parser::{iter_children, AudioTrack};
use super::stbl::parse_stbl;

/// 尝试从 trak body 解析 audio track。非 audio（如 video）返 None。
pub(super) fn try_parse_audio_trak(trak_body: &[u8]) -> Result<Option<AudioTrack>> {
    let mut mdia_body: Option<Vec<u8>> = None;
    iter_children(trak_body, |h, b| {
        if &h.box_type == b"mdia" {
            mdia_body = Some(b.to_vec());
        }
        Ok(())
    })?;
    let mdia = match mdia_body {
        Some(b) => b,
        None => return Ok(None),
    };
    parse_mdia(&mdia)
}

/// mdia 内有 mdhd + hdlr + minf。hdlr.handler_type 决定是不是 audio（"soun"）。
fn parse_mdia(mdia_body: &[u8]) -> Result<Option<AudioTrack>> {
    let mut mdhd_timescale: u32 = 0;
    let mut mdhd_duration: u64 = 0;
    let mut is_audio = false;
    let mut minf_body: Option<Vec<u8>> = None;
    iter_children(mdia_body, |h, b| {
        match &h.box_type {
            b"mdhd" => {
                let (ts, dur) = read_mdhd_ts_dur(b)?;
                mdhd_timescale = ts;
                mdhd_duration = dur;
            }
            // hdlr: version(1)+flags(3)+pre_defined(4)+handler_type(4)
            b"hdlr" if b.len() >= 12 && &b[8..12] == b"soun" => {
                is_audio = true;
            }
            b"minf" => minf_body = Some(b.to_vec()),
            _ => {}
        }
        Ok(())
    })?;
    if !is_audio {
        return Ok(None);
    }
    let minf = minf_body.ok_or_else(|| anyhow!("audio mdia 缺 minf"))?;
    let mut stbl_body: Option<Vec<u8>> = None;
    iter_children(&minf, |h, b| {
        if &h.box_type == b"stbl" {
            stbl_body = Some(b.to_vec());
        }
        Ok(())
    })?;
    let stbl = stbl_body.ok_or_else(|| anyhow!("audio minf 缺 stbl"))?;
    let mut t = parse_stbl(&stbl)?;
    t.mdhd_timescale = mdhd_timescale;
    t.mdhd_duration = mdhd_duration;
    Ok(Some(t))
}

/// mdhd body → (timescale, duration)。v0: ts@12 dur@16；v1: ts@20 dur@24。
fn read_mdhd_ts_dur(body: &[u8]) -> Result<(u32, u64)> {
    if body.is_empty() {
        anyhow::bail!("mdhd body 空");
    }
    let version = body[0];
    if version == 0 {
        if body.len() < 20 {
            anyhow::bail!("mdhd v0 截断");
        }
        let ts = u32::from_be_bytes(body[12..16].try_into().unwrap());
        let dur = u32::from_be_bytes(body[16..20].try_into().unwrap()) as u64;
        Ok((ts, dur))
    } else {
        if body.len() < 32 {
            anyhow::bail!("mdhd v1 截断");
        }
        let ts = u32::from_be_bytes(body[20..24].try_into().unwrap());
        let dur = u64::from_be_bytes(body[24..32].try_into().unwrap());
        Ok((ts, dur))
    }
}
