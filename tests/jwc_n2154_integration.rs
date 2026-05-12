//! N2154 周次课表端点集成测（用 T0 真机抓的脱敏 fixture）。
//!
//! 测试范围: fixture JSON → Schedule struct 反序列化 + 字段断言.
//! 不测真实 SJTU session / CAS 链路（那是 #[ignore] 真机 smoke test）.

use std::fs;

#[test]
fn fixture_n2154_zs1_can_be_parsed_as_schedule() {
    let raw = fs::read_to_string("tests/fixtures/jwc/n2154_week_zs1.json")
        .expect("fixture n2154_week_zs1.json 必须存在（T0 抓）");
    let s: sjtu_cli::apps::jwc::Schedule = serde_json::from_str(&raw).expect("Schedule 解析失败");
    // T0 抓的真实 zs=1 响应必带 rqazcList（第 1 周 7 天）
    assert!(
        !s.rqazc_list.is_empty(),
        "zs=1 响应必带 rqazcList（用于反推今天周次）"
    );
    // 第一天应是第 1 周周一 (xqj == 1, Option<u8>)
    assert_eq!(s.rqazc_list[0].xqj, Some(1));
}

#[test]
fn fixture_n2154_zs14_has_kb_list_with_old_zc_bits_for_week_14() {
    let raw = fs::read_to_string("tests/fixtures/jwc/n2154_week_zs14.json")
        .expect("fixture n2154_week_zs14.json 必须存在（T0 抓）");
    let s: sjtu_cli::apps::jwc::Schedule = serde_json::from_str(&raw).expect("Schedule 解析失败");
    // 至少一条课的 oldzc 在第 14 周（位 13）有 1
    let any_in_week14 = s.kb_list.iter().any(|k| {
        k.old_zc
            .map(|z| sjtu_cli::apps::jwc::period_clock::is_in_week(z, 14))
            .unwrap_or(false)
    });
    assert!(
        any_in_week14,
        "zs=14 响应应至少有 1 节课的 oldzc 在第 14 周（否则 fixture 不对）"
    );
}
