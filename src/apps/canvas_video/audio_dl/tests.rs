//! audio_dl 单元测试。

use super::client::build_client_pool_audio;

#[test]
fn build_client_pool_audio_accepts_valid_referer() {
    let c = build_client_pool_audio("https://courses.sjtu.edu.cn");
    assert!(c.is_ok(), "valid referer 应能构 client pool: {:?}", c.err());
}

#[test]
fn build_client_pool_audio_rejects_non_ascii_referer() {
    let c = build_client_pool_audio("https://例.cn");
    assert!(c.is_err(), "非 ASCII referer 应报错");
}

use mockito::Server;

const FIXTURE_STANDARD: &[u8] =
    include_bytes!("../../../../tests/fixtures/canvas_video/audio_1s_standard.mp4");

/// 在 mp4 字节里返回 moov box 起点（用于 mockito 测试断言）。
fn find_moov_offset(mp4: &[u8]) -> usize {
    let mut pos = 0usize;
    while pos + 8 <= mp4.len() {
        let size = u32::from_be_bytes(mp4[pos..pos + 4].try_into().unwrap()) as usize;
        if &mp4[pos + 4..pos + 8] == b"moov" {
            return pos;
        }
        if size == 0 {
            break;
        }
        pos += size;
    }
    panic!("fixture 没有 moov");
}

#[tokio::test]
async fn locate_moov_falls_back_to_tail_when_head_lacks_moov() {
    let mut server = Server::new_async().await;
    let total = FIXTURE_STANDARD.len();
    // HEAD probe (Range 0-0)：返 size
    let _m_probe = server
        .mock("GET", "/v.mp4")
        .match_header("range", "bytes=0-0")
        .with_status(206)
        .with_header("content-range", &format!("bytes 0-0/{total}"))
        .with_body(&FIXTURE_STANDARD[0..1])
        .create_async()
        .await;
    // 头部 1 MB（这里 fixture 才 ~1.3 KB，整个就是头部）
    let head_end = (1024 * 1024 - 1).min(total - 1);
    let _m_head = server
        .mock("GET", "/v.mp4")
        .match_header("range", &*format!("bytes=0-{head_end}"))
        .with_status(206)
        .with_header("content-range", &format!("bytes 0-{head_end}/{total}"))
        .with_body(&FIXTURE_STANDARD[..=head_end])
        .create_async()
        .await;
    // 尾部 1 MB（standard layout fallback 路径）
    let tail_start = total.saturating_sub(1024 * 1024);
    let _m_tail = server
        .mock("GET", "/v.mp4")
        .match_header("range", &*format!("bytes={tail_start}-{}", total - 1))
        .with_status(206)
        .with_header(
            "content-range",
            &format!("bytes {tail_start}-{}/{total}", total - 1),
        )
        .with_body(&FIXTURE_STANDARD[tail_start..])
        .create_async()
        .await;

    let url = format!("{}/v.mp4", server.url());
    let (moov_bytes, downloaded) =
        super::test_helpers::locate_moov_for_test(&url, "https://courses.sjtu.edu.cn")
            .await
            .expect("locate moov");
    let expected_offset = find_moov_offset(FIXTURE_STANDARD);
    let expected_size = u32::from_be_bytes(
        FIXTURE_STANDARD[expected_offset..expected_offset + 4]
            .try_into()
            .unwrap(),
    ) as usize;
    assert_eq!(
        moov_bytes.len(),
        expected_size,
        "moov 字节数应匹配 fixture 内 moov size"
    );
    assert!(downloaded > 0);
}

use super::ranges::merge_ranges;

#[test]
fn merge_ranges_collapses_adjacent_samples() {
    // 3 个紧邻 sample（gap=0），应合并成 1 个 Range
    let samples = vec![(100u64, 50u32), (150, 50), (200, 50)];
    let ranges = merge_ranges(&samples, 64 * 1024);
    assert_eq!(ranges, vec![(100, 249)]);
}

#[test]
fn merge_ranges_inlines_small_gap() {
    // gap = 50 字节 < 64KB 阈值 → 合并
    let samples = vec![(100u64, 50u32), (200, 50)];
    let ranges = merge_ranges(&samples, 64 * 1024);
    assert_eq!(ranges, vec![(100, 249)]);
}

#[test]
fn merge_ranges_splits_on_large_gap() {
    // gap = 100 KB > 64 KB → 不合并
    let samples = vec![(100u64, 50u32), (100 + 50 + 100 * 1024, 50)];
    let ranges = merge_ranges(&samples, 64 * 1024);
    assert_eq!(ranges.len(), 2);
}

#[test]
fn merge_ranges_handles_3000_samples() {
    // 3000 个 sample，每 sample 500B + 偶尔 100KB gap → 应合并到 < 100 个 Range
    let mut samples: Vec<(u64, u32)> = Vec::with_capacity(3000);
    let mut off = 0u64;
    for i in 0..3000 {
        samples.push((off, 500));
        off += 500;
        if i % 50 == 0 {
            off += 100 * 1024; // 100KB gap，会切
        }
    }
    let ranges = merge_ranges(&samples, 64 * 1024);
    assert!(
        ranges.len() < 100,
        "3000 sample 应合并到 < 100 Range，实际 {}",
        ranges.len()
    );
}

#[test]
fn merge_ranges_empty_input_returns_empty() {
    assert_eq!(merge_ranges(&[], 64 * 1024), vec![]);
}

#[test]
fn merge_ranges_splits_oversized_single_sample() {
    // 单 sample size > MAX_RANGE_SIZE：切多段
    let samples = vec![(0u64, 10 * 1024 * 1024u32)]; // 10 MB 单 sample
    let r = super::ranges::merge_ranges(&samples, 64 * 1024);
    assert_eq!(r.len(), 3); // 4 + 4 + 2 = 10 MB
    assert_eq!(r[0], (0, 4 * 1024 * 1024 - 1));
    assert_eq!(r[1], (4 * 1024 * 1024, 8 * 1024 * 1024 - 1));
    assert_eq!(r[2], (8 * 1024 * 1024, 10 * 1024 * 1024 - 1));
}

#[test]
fn merge_ranges_splits_merged_chunk() {
    // 多个紧邻 sample 合并 > MAX_RANGE_SIZE：切多段
    // 1000 × 8192 字节 = 8 192 000 字节（≈ 7.81 MB），紧邻无 gap
    let samples: Vec<(u64, u32)> = (0..1000).map(|i| (i * 8192u64, 8192u32)).collect();
    let r = super::ranges::merge_ranges(&samples, 64 * 1024);
    assert_eq!(r.len(), 2); // 4 MB + 剩余 ~3.81 MB
    assert_eq!(r[0].1 - r[0].0 + 1, 4 * 1024 * 1024);
    assert_eq!(r[0], (0, 4 * 1024 * 1024 - 1));
    assert_eq!(r[1].0, 4 * 1024 * 1024);
    assert_eq!(r[1].1, 8_192_000 - 1);
}

#[test]
fn merge_ranges_under_max_unchanged() {
    // 合并段 < MAX_RANGE_SIZE：原样返回
    let samples = vec![(0u64, 1024u32), (2048, 1024), (4096, 1024)];
    let r = super::ranges::merge_ranges(&samples, 64 * 1024);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0], (0, 5119));
}

#[test]
fn next_box_after_head_ftyp_then_huge_mdat() {
    // ftyp(24) + mdat 头(8, size=1_000_000)：head 只覆盖 ftyp + mdat 头
    // → next box 绝对起点 = 24 + 1_000_000 = 1_000_024
    let mut head = Vec::new();
    // ftyp: size=24, type='ftyp', minor + compat (16 字节内容)
    head.extend_from_slice(&24u32.to_be_bytes());
    head.extend_from_slice(b"ftyp");
    head.extend_from_slice(&[0u8; 16]);
    // mdat 头: size=1_000_000, type='mdat'
    head.extend_from_slice(&1_000_000u32.to_be_bytes());
    head.extend_from_slice(b"mdat");
    // 不补 mdat body（模拟 head 1MB 只截到 mdat 头）
    let r = super::locate::next_box_after_head_for_test(&head);
    assert_eq!(r, Some(1_000_024));
}

#[test]
fn next_box_after_head_box_size_zero_means_eof() {
    // ftyp + 一个 size=0 box（伸展到 EOF）→ None
    let mut head = Vec::new();
    head.extend_from_slice(&24u32.to_be_bytes());
    head.extend_from_slice(b"ftyp");
    head.extend_from_slice(&[0u8; 16]);
    head.extend_from_slice(&0u32.to_be_bytes()); // size=0
    head.extend_from_slice(b"mdat");
    let r = super::locate::next_box_after_head_for_test(&head);
    assert_eq!(r, None);
}

#[test]
fn next_box_after_head_all_within_head() {
    // 全部 box 都在 head 内，最后 pos == head.len() → Some(pos)
    let mut head = Vec::new();
    head.extend_from_slice(&8u32.to_be_bytes());
    head.extend_from_slice(b"free");
    head.extend_from_slice(&8u32.to_be_bytes());
    head.extend_from_slice(b"free");
    let r = super::locate::next_box_after_head_for_test(&head);
    assert_eq!(r, Some(16));
}

#[tokio::test]
#[ignore = "inter-byte abort: 35s 真等待，默认 cargo test 跳；--ignored 才跑"]
async fn fetch_range_aborts_on_inter_byte_timeout() {
    let mut server = Server::new_async().await;
    // 模拟连接 OK 但 chunk 间隔超过 30s（>= INTER_BYTE_TIMEOUT）
    let _m = server
        .mock("GET", "/slow")
        .with_status(206)
        .with_header("content-range", "bytes 0-9/10")
        .with_chunked_body(|w| {
            // 先写 1 字节，sleep 35s（> 30s 超时阈值），再写剩下
            w.write_all(b"X")?;
            std::thread::sleep(std::time::Duration::from_secs(35));
            w.write_all(b"YYYYYYYYY")?;
            Ok(())
        })
        .create_async()
        .await;
    let url = format!("{}/slow", server.url());
    let started = std::time::Instant::now();
    let result =
        super::test_helpers::fetch_range_for_test(&url, "https://courses.sjtu.edu.cn", 0, 9).await;
    let elapsed = started.elapsed();
    assert!(result.is_err(), "应在 30 s 触发 abort：{:?}", result);
    assert!(
        elapsed >= std::time::Duration::from_secs(28)
            && elapsed <= std::time::Duration::from_secs(45),
        "abort 应在 ~30 s（实际 {:?}）",
        elapsed
    );
    let msg = format!("{:#}", result.unwrap_err());
    assert!(
        msg.contains("30s 无字节流入"),
        "错误信息应提示 inter-byte：{msg}"
    );
}
