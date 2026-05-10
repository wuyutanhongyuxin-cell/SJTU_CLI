//! audio_dl 单元测试。

use super::client::build_client_audio;

#[test]
fn build_client_audio_accepts_valid_referer() {
    let c = build_client_audio("https://courses.sjtu.edu.cn");
    assert!(c.is_ok(), "valid referer 应能构 client: {:?}", c.err());
}

#[test]
fn build_client_audio_rejects_non_ascii_referer() {
    let c = build_client_audio("https://例.cn");
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
    use super::orchestrator::locate_moov_for_test;
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
    let (moov_bytes, downloaded) = locate_moov_for_test(&url, "https://courses.sjtu.edu.cn")
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
