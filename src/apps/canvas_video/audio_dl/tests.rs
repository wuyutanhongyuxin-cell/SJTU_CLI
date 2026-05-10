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
