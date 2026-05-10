//! build_moov：构造最小 moov box（mvhd + trak + mdia + minf + stbl）。
//! stco 字段先占位全 0，由 mod.rs 的 rewrite_stco 回填。

use crate::apps::canvas_video::mp4_box::AudioTrack;
use anyhow::Result;

/// 把 `body` 包成 `[size(4)][type(4)][body]` 格式。
pub(super) fn wrap_box(box_type: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let size = (8 + body.len()) as u32;
    let mut v = Vec::with_capacity(8 + body.len());
    v.extend_from_slice(&size.to_be_bytes());
    v.extend_from_slice(box_type);
    v.extend_from_slice(body);
    v
}

/// 构造 moov box（含 stco 占位 0）。
pub(super) fn build_moov(t: &AudioTrack) -> Result<Vec<u8>> {
    let stbl = build_stbl(t);
    let minf = build_minf(&stbl);
    let mdia = build_mdia(t, &minf);
    let trak = build_trak(t, &mdia);
    let mvhd = build_mvhd(t);

    let mut moov_body = Vec::new();
    moov_body.extend_from_slice(&mvhd);
    moov_body.extend_from_slice(&trak);
    Ok(wrap_box(b"moov", &moov_body))
}

fn build_stbl(t: &AudioTrack) -> Vec<u8> {
    let stsd = wrap_box(b"stsd", &t.stsd_raw);

    // stts: 全 sample 用 delta=1024（AAC 1 帧 1024 sample）
    let mut stts_body = Vec::new();
    stts_body.extend_from_slice(&[0, 0, 0, 0]);
    stts_body.extend_from_slice(&1u32.to_be_bytes());
    stts_body.extend_from_slice(&(t.sample_sizes.len() as u32).to_be_bytes());
    stts_body.extend_from_slice(&1024u32.to_be_bytes());
    let stts = wrap_box(b"stts", &stts_body);

    // stsc: 1 sample/chunk（简化，每 sample 一个 chunk）
    let mut stsc_body = Vec::new();
    stsc_body.extend_from_slice(&[0, 0, 0, 0]);
    stsc_body.extend_from_slice(&1u32.to_be_bytes());
    stsc_body.extend_from_slice(&1u32.to_be_bytes()); // first_chunk=1
    stsc_body.extend_from_slice(&1u32.to_be_bytes()); // samples_per_chunk=1
    stsc_body.extend_from_slice(&1u32.to_be_bytes()); // sample_desc_idx=1
    let stsc = wrap_box(b"stsc", &stsc_body);

    // stsz: sample_size=0（variable），后跟 entry 表
    let mut stsz_body = Vec::new();
    stsz_body.extend_from_slice(&[0, 0, 0, 0]);
    stsz_body.extend_from_slice(&0u32.to_be_bytes());
    stsz_body.extend_from_slice(&(t.sample_sizes.len() as u32).to_be_bytes());
    for &sz in &t.sample_sizes {
        stsz_body.extend_from_slice(&sz.to_be_bytes());
    }
    let stsz = wrap_box(b"stsz", &stsz_body);

    // stco: 占位（全 0），rewrite_stco 回填
    let mut stco_body = Vec::new();
    stco_body.extend_from_slice(&[0, 0, 0, 0]);
    stco_body.extend_from_slice(&(t.sample_sizes.len() as u32).to_be_bytes());
    for _ in 0..t.sample_sizes.len() {
        stco_body.extend_from_slice(&0u32.to_be_bytes());
    }
    let stco = wrap_box(b"stco", &stco_body);

    let mut stbl_body = Vec::new();
    stbl_body.extend_from_slice(&stsd);
    stbl_body.extend_from_slice(&stts);
    stbl_body.extend_from_slice(&stsc);
    stbl_body.extend_from_slice(&stsz);
    stbl_body.extend_from_slice(&stco);
    wrap_box(b"stbl", &stbl_body)
}

fn build_minf(stbl: &[u8]) -> Vec<u8> {
    // smhd（sound media header）
    let smhd_body: &[u8] = &[0, 0, 0, 0, 0, 0, 0, 0];
    let smhd = wrap_box(b"smhd", smhd_body);

    // dinf / dref / url（self-contained）
    let dref_body: &[u8] = &[
        0, 0, 0, 0, 0, 0, 0, 1, // version+flags, entry_count=1
        0, 0, 0, 12, // size=12
        b'u', b'r', b'l', b' ', 0, 0, 0, 1, // url flags=1（self-contained）
    ];
    let dref = wrap_box(b"dref", dref_body);
    let dinf = wrap_box(b"dinf", &dref);

    let mut minf_body = Vec::new();
    minf_body.extend_from_slice(&smhd);
    minf_body.extend_from_slice(&dinf);
    minf_body.extend_from_slice(stbl);
    wrap_box(b"minf", &minf_body)
}

fn build_mdia(t: &AudioTrack, minf: &[u8]) -> Vec<u8> {
    // mdhd v0
    let dur32 = (t.mdhd_duration.min(u32::MAX as u64)) as u32;
    let mut mdhd_body = Vec::new();
    mdhd_body.extend_from_slice(&[0, 0, 0, 0]);
    mdhd_body.extend_from_slice(&[0, 0, 0, 0]); // ctime
    mdhd_body.extend_from_slice(&[0, 0, 0, 0]); // mtime
    mdhd_body.extend_from_slice(&t.mdhd_timescale.to_be_bytes());
    mdhd_body.extend_from_slice(&dur32.to_be_bytes());
    mdhd_body.extend_from_slice(&[0x55, 0xc4]); // language='und'
    mdhd_body.extend_from_slice(&[0, 0]);
    let mdhd = wrap_box(b"mdhd", &mdhd_body);

    // hdlr (handler='soun')
    let mut hdlr_body = Vec::new();
    hdlr_body.extend_from_slice(&[0, 0, 0, 0]);
    hdlr_body.extend_from_slice(&[0, 0, 0, 0]); // pre_defined
    hdlr_body.extend_from_slice(b"soun");
    hdlr_body.extend_from_slice(&[0u8; 12]); // reserved
    hdlr_body.extend_from_slice(b"sjtu\0");
    let hdlr = wrap_box(b"hdlr", &hdlr_body);

    let mut mdia_body = Vec::new();
    mdia_body.extend_from_slice(&mdhd);
    mdia_body.extend_from_slice(&hdlr);
    mdia_body.extend_from_slice(minf);
    wrap_box(b"mdia", &mdia_body)
}

fn build_trak(t: &AudioTrack, mdia: &[u8]) -> Vec<u8> {
    let dur32 = (t.mdhd_duration.min(u32::MAX as u64)) as u32;
    let matrix: [i32; 9] = [0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000];

    let mut tkhd_body = Vec::new();
    tkhd_body.extend_from_slice(&[0, 0, 0, 7]); // version=0, flags=enabled+in_movie+in_preview
    tkhd_body.extend_from_slice(&[0, 0, 0, 0]); // ctime
    tkhd_body.extend_from_slice(&[0, 0, 0, 0]); // mtime
    tkhd_body.extend_from_slice(&1u32.to_be_bytes()); // track_id
    tkhd_body.extend_from_slice(&[0, 0, 0, 0]); // reserved
    tkhd_body.extend_from_slice(&dur32.to_be_bytes());
    tkhd_body.extend_from_slice(&[0u8; 8]); // reserved
    tkhd_body.extend_from_slice(&[0, 0]); // layer
    tkhd_body.extend_from_slice(&[0, 1]); // alternate_group=1
    tkhd_body.extend_from_slice(&[1, 0]); // volume=1.0 (8.8 fixed)
    tkhd_body.extend_from_slice(&[0, 0]); // reserved
    for v in matrix {
        tkhd_body.extend_from_slice(&v.to_be_bytes());
    }
    tkhd_body.extend_from_slice(&[0, 0, 0, 0]); // width
    tkhd_body.extend_from_slice(&[0, 0, 0, 0]); // height
    let tkhd = wrap_box(b"tkhd", &tkhd_body);

    let mut trak_body = Vec::new();
    trak_body.extend_from_slice(&tkhd);
    trak_body.extend_from_slice(mdia);
    wrap_box(b"trak", &trak_body)
}

fn build_mvhd(t: &AudioTrack) -> Vec<u8> {
    let dur32 = (t.mdhd_duration.min(u32::MAX as u64)) as u32;
    let mvhd_ts = if t.mvhd_timescale == 0 {
        1000
    } else {
        t.mvhd_timescale
    };
    let matrix: [i32; 9] = [0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000];

    let mut mvhd_body = Vec::new();
    mvhd_body.extend_from_slice(&[0, 0, 0, 0]);
    mvhd_body.extend_from_slice(&[0, 0, 0, 0]); // ctime
    mvhd_body.extend_from_slice(&[0, 0, 0, 0]); // mtime
    mvhd_body.extend_from_slice(&mvhd_ts.to_be_bytes());
    mvhd_body.extend_from_slice(&dur32.to_be_bytes());
    mvhd_body.extend_from_slice(&[0, 1, 0, 0]); // rate=1.0
    mvhd_body.extend_from_slice(&[1, 0]); // volume=1.0
    mvhd_body.extend_from_slice(&[0u8; 10]); // reserved
    for v in matrix {
        mvhd_body.extend_from_slice(&v.to_be_bytes());
    }
    mvhd_body.extend_from_slice(&[0u8; 24]); // pre_defined
    mvhd_body.extend_from_slice(&2u32.to_be_bytes()); // next_track_id
    wrap_box(b"mvhd", &mvhd_body)
}
