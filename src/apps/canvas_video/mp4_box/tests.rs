//! mp4_box 单元测试：box header 读取边界（fixture mp4 测试见 Task 2）。

use super::parser::{parse_moov, read_box_header};

#[test]
fn read_box_header_parses_size_and_type() {
    // box: size=12（含自身 8 字节 header）, type=ftyp, body 4 字节
    let bytes = [
        0x00, 0x00, 0x00, 0x0c, // size = 12
        b'f', b't', b'y', b'p', // type = ftyp
        0xde, 0xad, 0xbe, 0xef,
    ];
    let h = read_box_header(&bytes, 0).unwrap();
    assert_eq!(h.size, 12);
    assert_eq!(h.box_type, *b"ftyp");
    assert_eq!(h.header_len, 8);
    assert_eq!(h.body_start, 8);
}

#[test]
fn read_box_header_handles_largesize_64bit() {
    // size=1 → 后面 8 字节是真 size（large box）
    let bytes = [
        0x00, 0x00, 0x00, 0x01, // size = 1（信号）
        b'm', b'd', b'a', b't', 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
        0x00, // largesize = 4096
    ];
    let h = read_box_header(&bytes, 0).unwrap();
    assert_eq!(h.size, 4096);
    assert_eq!(h.box_type, *b"mdat");
    assert_eq!(h.header_len, 16);
    assert_eq!(h.body_start, 16);
}

#[test]
fn read_box_header_rejects_truncated_input() {
    let bytes = [0x00, 0x00, 0x00, 0x0c, b'f', b't']; // 只 6 字节，header 至少 8
    assert!(read_box_header(&bytes, 0).is_err());
}

#[test]
fn read_box_header_handles_pos_beyond_buf_len() {
    // pos > buf.len() — caller miscalculated. Must Err gracefully, not panic.
    let bytes = [0u8; 4];
    let result = read_box_header(&bytes, 100);
    assert!(result.is_err(), "pos > buf.len() 应 Err 而不 panic");
}

const FIXTURE_FASTSTART: &[u8] =
    include_bytes!("../../../../tests/fixtures/canvas_video/audio_1s_faststart.mp4");

/// 从整个 mp4 文件字节里把 moov box 字节切出来（顺序扫，遇到 type=moov 即返）。
fn extract_moov_bytes(mp4: &[u8]) -> Vec<u8> {
    let mut pos = 0usize;
    while pos + 8 <= mp4.len() {
        let h = read_box_header(mp4, pos).unwrap();
        let end = pos + h.size as usize;
        if &h.box_type == b"moov" {
            return mp4[pos..end].to_vec();
        }
        pos = end;
    }
    panic!("fixture 没找到 moov box");
}

#[test]
fn parse_moov_handles_truncated_input() {
    // moov header claims size 1024 but buffer only has the 8-byte header.
    let bytes = [
        0x00, 0x00, 0x04, 0x00, // size = 1024
        b'm', b'o', b'o', b'v',
    ];
    let r = parse_moov(&bytes);
    assert!(r.is_err());
    let msg = format!("{}", r.unwrap_err());
    assert!(
        msg.contains("超出缓冲区"),
        "应报 moov size 超出 buffer：{msg}"
    );
}

#[test]
fn parse_moov_faststart_extracts_aac_track() {
    let moov = extract_moov_bytes(FIXTURE_FASTSTART);
    let track = parse_moov(&moov).expect("parse moov");
    assert_eq!(track.codec, "mp4a");
    assert_eq!(track.channels, 2);
    assert_eq!(track.sample_rate, 44100);
    assert!(track.mvhd_timescale > 0, "mvhd_timescale 必非 0");
    assert!(track.mdhd_timescale > 0, "mdhd_timescale 必非 0");
    assert!(!track.stsd_raw.is_empty(), "stsd_raw 必非空");
    // 1 秒 AAC @ 44.1kHz 通常 ~43 个 sample（1024 sample/frame × 43 ≈ 44032 → ~1s）
    assert!(
        track.sample_sizes.len() >= 30 && track.sample_sizes.len() <= 60,
        "1s 音频 sample 数应在 30-60 范围: {}",
        track.sample_sizes.len()
    );
    assert_eq!(track.sample_offsets.len(), track.sample_sizes.len());
}

const FIXTURE_STANDARD: &[u8] =
    include_bytes!("../../../../tests/fixtures/canvas_video/audio_1s_standard.mp4");

#[test]
fn parse_moov_standard_layout_extracts_aac_track() {
    let moov = extract_moov_bytes(FIXTURE_STANDARD);
    let track = parse_moov(&moov).expect("parse moov standard");
    assert_eq!(track.codec, "mp4a");
    assert_eq!(track.channels, 2);
    assert_eq!(track.sample_rate, 44100);
    // standard layout sample offset 都在 ftyp+free 之后、moov 之前的 mdat 区域 → 比 faststart 小
    let max_off = *track.sample_offsets.iter().max().unwrap();
    assert!(max_off > 0, "sample offset 必非 0");
    // 每个 sample 大小至少 1 字节
    assert!(track.sample_sizes.iter().all(|&s| s > 0));
    // 总字节合理范围（1 秒 AAC 64kbps ≈ 8 KB；fixture 是无声源 64kbps，实际更小）
    let total: u64 = track.sample_sizes.iter().map(|&s| s as u64).sum();
    assert!(
        (10..30_000).contains(&total),
        "sample 总字节应 10-30000 范围: {total}"
    );
}
