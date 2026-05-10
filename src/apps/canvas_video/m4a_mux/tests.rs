//! m4a_mux 单元测试：parse fixture → 抽 sample bytes → mux → re-parse 应等价。

use crate::apps::canvas_video::mp4_box::{extract_moov_bytes_for_test, parse_moov};

use super::write_m4a;

const FIXTURE_FASTSTART: &[u8] =
    include_bytes!("../../../../tests/fixtures/canvas_video/audio_1s_faststart.mp4");

/// 从原 mp4 字节里把 audio sample bytes 按 sample_offsets 顺序拼出来（紧密排列）。
fn collect_sample_bytes(mp4: &[u8], offsets: &[u64], sizes: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    for (&off, &sz) in offsets.iter().zip(sizes.iter()) {
        let s = off as usize;
        let e = s + sz as usize;
        out.extend_from_slice(&mp4[s..e]);
    }
    out
}

#[tokio::test]
async fn write_m4a_round_trip_preserves_codec_and_sample_count() {
    let moov = extract_moov_bytes_for_test(FIXTURE_FASTSTART);
    let track = parse_moov(&moov).expect("parse fixture moov");
    let sample_bytes = collect_sample_bytes(
        FIXTURE_FASTSTART,
        &track.sample_offsets,
        &track.sample_sizes,
    );

    let tmp = std::env::temp_dir().join("sjtu_v5d_round_trip.m4a");
    let _ = std::fs::remove_file(&tmp);
    let written = write_m4a(&tmp, &track, &sample_bytes).expect("write m4a");
    assert!(written > 0);

    // re-parse：用 tokio fs 读回
    let back = tokio::fs::read(&tmp).await.expect("read m4a back");
    let new_moov = extract_moov_bytes_for_test(&back);
    let new_track = parse_moov(&new_moov).expect("parse round-trip m4a");
    assert_eq!(new_track.codec, "mp4a");
    assert_eq!(new_track.channels, track.channels);
    assert_eq!(new_track.sample_rate, track.sample_rate);
    assert_eq!(new_track.sample_sizes.len(), track.sample_sizes.len());

    let _ = std::fs::remove_file(&tmp);
}
