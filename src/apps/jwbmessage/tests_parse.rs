//! apps::jwbmessage 响应解析单测 + throttle 间隔单测。
//!
//! fixture 精简自真实抓包（tasks/s3b-jiaowoban-messages.md §2），字段语义字字对齐。
//! HTTP 路径（URL / header / error 分支）靠 tests_write.rs + 真机 CP-M1/CP-M2 验证。

use super::models::{GroupEnvelope, MessageListEnvelope, ReadAllResponse, UnreadNum};
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
fn parse_unread_num_basic() {
    // 真实抓包：{"total":108,"errno":0,"entities":[],"error":null}
    let fixture = r#"{"total":108,"errno":0,"entities":[],"error":null}"#;
    let u: UnreadNum = serde_json::from_str(fixture).unwrap();
    assert_eq!(u.total, 108);
    assert_eq!(u.errno, 0);
}

#[test]
fn parse_unread_num_tolerates_missing_errno() {
    let fixture = r#"{"total":0}"#;
    let u: UnreadNum = serde_json::from_str(fixture).unwrap();
    assert_eq!(u.total, 0);
}

#[test]
fn parse_group_envelope_with_mixed_group_id_shapes() {
    // 真实抓包：groupId 形态多样（UUID / 短串 / 数字 / 应用 key），全按 String 处理。
    let fixture = r#"{
      "success":true,"errno":0,"message":"success","total":15,
      "entities":[
        {"groupId":"AGM4V5mqXn2PDwznSErB","groupName":"Canvas日历通知","unreadNum":4,"groupDescription":"…","isGroup":false,"isRead":true,"icon":"https://api.sjtu.edu.cn/v1/file/x","createTime":"2026-04-24 09:00:46"},
        {"groupId":"EB6A023D-1234-5678-9ABC-DEADBEEFCAFE","groupName":"教务处","unreadNum":0,"isGroup":false,"isRead":true,"icon":null,"createTime":"2026-04-23 10:00:00"},
        {"groupId":"50500","groupName":"数字人通知","unreadNum":1,"isGroup":true,"isRead":false},
        {"groupId":"jaform101110","groupName":"表单系统","unreadNum":2,"isGroup":false,"isRead":false}
      ],
      "entity":null
    }"#;
    let env: GroupEnvelope = serde_json::from_str(fixture).unwrap();
    assert_eq!(env.total, 15);
    assert_eq!(env.entities.len(), 4);
    assert_eq!(env.entities[0].group_id, "AGM4V5mqXn2PDwznSErB");
    assert_eq!(env.entities[0].group_name, "Canvas日历通知");
    assert_eq!(env.entities[0].unread_num, 4);
    assert!(env.entities[0].is_read);
    assert_eq!(env.entities[2].group_id, "50500");
    assert!(env.entities[2].is_group);
    assert_eq!(env.entities[3].group_id, "jaform101110");
    // icon 可 null / 缺失。
    assert_eq!(env.entities[1].icon, None);
}

#[test]
fn parse_message_list_envelope_with_auth_client_and_context() {
    let fixture = r#"{
      "success":true,"errno":0,"message":"success","total":23,
      "entities":[
        {
          "messageId":"f88610ba-d463-4ffc-8e8a-4370177b53e5",
          "type":"basic",
          "title":null,
          "description":"班车预约成功",
          "readTime":null,
          "read":false,
          "expireTime":null,
          "notificationId":"75033ea2-318f-11f1-8860-fa163ecd40a6",
          "createTime":"2026-04-06 16:06:04",
          "pushTitle":"班车预约成功",
          "pushContent":"完整正文内容\n第二行",
          "authClient":{"name":"学生预约乘车","apiKey":"gD7xfTi3zhrAt94Njg7o","description":"校区间通勤班车预约管理系统。","iconUrl":"https://api.sjtu.edu.cn/v1/file/x"},
          "picture":null,
          "urlList":null,
          "context":[{"key":"内容","value":"校区班车预约成功..."}]
        },
        {
          "messageId":"aaa-bbb",
          "type":"basic",
          "read":true,
          "readTime":"2026-04-20 10:00:00",
          "pushContent":"已读消息"
        }
      ],
      "entity":null
    }"#;
    let env: MessageListEnvelope = serde_json::from_str(fixture).unwrap();
    assert_eq!(env.total, 23);
    assert_eq!(env.entities.len(), 2);
    let m0 = &env.entities[0];
    assert_eq!(m0.message_id, "f88610ba-d463-4ffc-8e8a-4370177b53e5");
    assert_eq!(m0.kind.as_deref(), Some("basic"));
    assert!(!m0.read);
    assert_eq!(m0.push_title.as_deref(), Some("班车预约成功"));
    assert!(m0.push_content.as_deref().unwrap().contains("第二行"));
    let ac = m0.auth_client.as_ref().expect("authClient 必填");
    assert_eq!(ac.name.as_deref(), Some("学生预约乘车"));
    assert_eq!(ac.api_key.as_deref(), Some("gD7xfTi3zhrAt94Njg7o"));
    let ctx = m0.context.as_ref().expect("context 必填");
    assert_eq!(ctx.len(), 1);
    assert_eq!(ctx[0].key, "内容");
    // 第二条：大量字段缺失，应被 `#[serde(default)]` 兜底。
    let m1 = &env.entities[1];
    assert!(m1.read);
    assert_eq!(m1.read_time.as_deref(), Some("2026-04-20 10:00:00"));
    assert!(m1.auth_client.is_none());
    assert!(m1.context.is_none());
    assert!(m1.description.is_none());
}

#[test]
fn parse_empty_group_list_tolerates() {
    let fixture = r#"{"success":true,"total":0,"entities":[],"entity":null}"#;
    let env: GroupEnvelope = serde_json::from_str(fixture).unwrap();
    assert_eq!(env.total, 0);
    assert_eq!(env.entities.len(), 0);
}

#[test]
fn parse_read_all_response_tolerates_shapes() {
    // 未实测响应形状 —— 兼容以下三种：有 errno / 只 success / 空字段。
    let a: ReadAllResponse =
        serde_json::from_str(r#"{"errno":0,"success":true,"message":"ok"}"#).unwrap();
    assert_eq!(a.errno, 0);
    assert!(a.success);

    let b: ReadAllResponse = serde_json::from_str(r#"{"success":true}"#).unwrap();
    assert!(b.success);
    assert_eq!(b.errno, 0);

    let c: ReadAllResponse = serde_json::from_str(r#"{}"#).unwrap();
    assert!(!c.success);
    assert_eq!(c.errno, 0);
    assert_eq!(c.message, None);
}
