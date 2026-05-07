//! `apps::elec` 响应解析 + 节流单测。
//!
//! fixture 精简自真实抓包（§7.3 sydl / §7.4 ydl / §7.5 ydlmx），字段语义对齐。
//! HTTP 路径（CAS / URL / header / 错误分支）靠真机 CP-E4 验证。

use rust_decimal::Decimal;
use std::str::FromStr;

use super::models::{Balance, DailyUsage, Envelope, MeInfo, Monthly};
use super::throttle::{Throttle, MIN_INTERVAL};

#[tokio::test]
async fn throttle_enforces_minimum_interval() {
    let t = Throttle::new();
    t.wait().await;
    let t0 = std::time::Instant::now();
    t.wait().await;
    let elapsed = t0.elapsed();
    // Windows 下 tokio::time::sleep 精度 ±15ms，下界放宽。
    assert!(
        elapsed >= MIN_INTERVAL.saturating_sub(std::time::Duration::from_millis(15)),
        "节流间隔过短：{elapsed:?}，期望 >= {MIN_INTERVAL:?}"
    );
}

#[test]
fn parse_balance_string_money() {
    // §7.3 真实抓包：4 个字段都是字符串
    let fixture = r#"{
      "errno":0,"error":"success","total":1,
      "entities":[{"SYL":"284.25","SYBZ":"18.22","SYLJE":"180.78","SYBZJE":"11.59"}]
    }"#;
    let env: Envelope<Balance> = serde_json::from_str(fixture).unwrap();
    assert_eq!(env.entities.len(), 1);
    let b = env.entities.first().expect("应有 1 条");
    assert_eq!(b.remaining_kwh, Decimal::from_str("284.25").unwrap());
    assert_eq!(b.subsidy_kwh, Decimal::from_str("18.22").unwrap());
    assert_eq!(b.balance_yuan, Decimal::from_str("180.78").unwrap());
    assert_eq!(b.subsidy_balance_yuan, Decimal::from_str("11.59").unwrap());
}

#[test]
fn parse_monthly_number_money_total_zero_quirk() {
    // §7.4 真实抓包：last/now 是 number，total=0 是服务端误标
    let fixture = r#"{
      "errno":0,"error":"success","total":0,
      "entities":[{"last":80.55,"now":62.44}]
    }"#;
    let env: Envelope<Monthly> = serde_json::from_str(fixture).unwrap();
    // §7.6 服务端误标 `total:0`（fixture 顶层），CLI 不读 total，只看 entities
    assert_eq!(env.entities.len(), 1);
    let m = env.entities.first().unwrap();
    assert_eq!(m.last, Decimal::from_str("80.55").unwrap());
    assert_eq!(m.now, Decimal::from_str("62.44").unwrap());
}

#[test]
fn parse_daily_history_number_money() {
    // §7.5 真实抓包：used 是 number
    let fixture = r#"{
      "errno":0,"error":"success","total":2,
      "entities":[
        {"roomdm":"02100406","date":"2026-04-24","used":3.37},
        {"roomdm":"02100406","date":"2026-04-25","used":2.16}
      ]
    }"#;
    let env: Envelope<DailyUsage> = serde_json::from_str(fixture).unwrap();
    assert_eq!(env.entities.len(), 2);
    assert_eq!(env.entities[0].used, Decimal::from_str("3.37").unwrap());
    assert_eq!(env.entities[1].date, "2026-04-25");
}

#[test]
fn parse_me_info_redact_targets_present() {
    // §7.1 假数据（不含真实姓名/工号）：验证 isbind / roomname 等关键字段
    let fixture = r#"{
      "errno":0,"error":"success","total":1,
      "entities":[{
        "account":"jaccount-x","name":"张三","workNo":"5xxxxxxxxxxxx",
        "deptcode":"14000","dept":"某学院","userType":"student",
        "isbind":true,"xqdm":"02","lddm":"0210","roomdm":"02100406",
        "roomid":11226,"roomname":"D10-406","mdid":"11452"
      }]
    }"#;
    let env: Envelope<MeInfo> = serde_json::from_str(fixture).unwrap();
    let me = env.entities.first().unwrap();
    assert!(me.isbind);
    assert_eq!(me.roomname.as_deref(), Some("D10-406"));
    assert_eq!(me.roomdm.as_deref(), Some("02100406"));
    assert_eq!(me.mdid.as_deref(), Some("11452"));
    // 身份字段反序列化进了 struct，但命令层会丢弃 —— 这里只断言能拿到
    assert!(me.name.is_some());
    assert!(me.work_no.is_some());
}

#[test]
fn balance_serializes_decimal_as_string() {
    // 序列化输出必须是字符串而不是 JSON number（避开 f64 精度坑）
    let b = Balance {
        remaining_kwh: Decimal::from_str("284.25").unwrap(),
        subsidy_kwh: Decimal::from_str("18.22").unwrap(),
        balance_yuan: Decimal::from_str("180.78").unwrap(),
        subsidy_balance_yuan: Decimal::from_str("11.59").unwrap(),
    };
    let json = serde_json::to_string(&b).unwrap();
    assert!(json.contains(r#""SYL":"284.25""#), "got: {json}");
    assert!(json.contains(r#""SYLJE":"180.78""#), "got: {json}");
}

#[test]
fn empty_entities_tolerated() {
    let fixture = r#"{"errno":0,"error":"success","total":0,"entities":[]}"#;
    let env: Envelope<Balance> = serde_json::from_str(fixture).unwrap();
    assert!(env.entities.is_empty());
}
