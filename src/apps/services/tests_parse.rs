//! apps::services 响应解析单测 + throttle 间隔单测。
//!
//! fixture 精简自真实抓包（tasks/isjtu_investigation.md §6.3），字段语义对齐。
//! HTTP 路径（URL / header / error 分支）靠真机 CP-D4 验证。

use super::models::{TodoEnvelope, TodoItem};
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
fn parse_todo_envelope_with_nested_process() {
    // 真实抓包结构（§6.3）：1 条待办，code=ADD（我申请的），含完整 process 子对象。
    let fixture = r#"{
      "errno":0,"error":"","total":1,
      "entities":[
        {
          "id":"0964f041-bc0f-4640-bd07-de9151228bc8",
          "name":"填写申请",
          "code":"ADD",
          "uri":"https://form.sjtu.edu.cn/infoplus/form/36790604/render",
          "assignTime":1773540888,
          "status":1,
          "process":{
            "id":"abc-123",
            "entry":"20054472",
            "name":"户外活动报备申请",
            "uri":"https://form.sjtu.edu.cn/infoplus/process/123",
            "create":1773540000,
            "update":1773540888,
            "status":"doing",
            "version":1700000000,
            "app":{
              "code":"HXBDSQ",
              "name":"户外活动报备",
              "department":"学生处",
              "contact":"021-12345678 / abc@sjtu.edu.cn",
              "tags":"学生服务,#CategoryName:学工,#CategoryOrder:3,#CategoryColumns:2"
            },
            "owner":{
              "account":"zhangsan",
              "name":"张三",
              "id":"uuid-owner-1"
            },
            "milestone":{"percent":25}
          }
        }
      ]
    }"#;
    let env: TodoEnvelope = serde_json::from_str(fixture).unwrap();
    assert_eq!(env.total, 1);
    assert_eq!(env.entities.len(), 1);

    let it = &env.entities[0];
    assert_eq!(it.id, "0964f041-bc0f-4640-bd07-de9151228bc8");
    assert_eq!(it.code.as_deref(), Some("ADD"));
    assert_eq!(it.name.as_deref(), Some("填写申请"));
    assert_eq!(it.assign_time, Some(1773540888));
    assert_eq!(it.status, Some(1));

    let p = it.process.as_ref().expect("process 必填");
    assert_eq!(p.entry.as_deref(), Some("20054472"));
    assert_eq!(p.name.as_deref(), Some("户外活动报备申请"));
    assert_eq!(p.status.as_deref(), Some("doing"));

    let app = p.app.as_ref().expect("app 必填");
    assert_eq!(app.code.as_deref(), Some("HXBDSQ"));
    // tags 透传（含 # 元信息）—— MVP 不过滤
    assert!(app.tags.as_deref().unwrap().contains("#CategoryName"));

    let owner = p.owner.as_ref().expect("owner 必填");
    assert_eq!(owner.account.as_deref(), Some("zhangsan"));
    assert_eq!(owner.name.as_deref(), Some("张三"));
    assert_eq!(owner.id.as_deref(), Some("uuid-owner-1"));

    let m = p.milestone.as_ref().expect("milestone 必填");
    assert_eq!(m.percent, Some(25));
}

#[test]
fn parse_todo_tolerates_missing_optional_fields() {
    // 等我处理的 / 通知服务平台类项：大量字段缺失，全靠 #[serde(default)] 兜底。
    let fixture = r#"{
      "errno":0,"error":"","total":1,
      "entities":[
        {
          "id":"step-2",
          "name":"审核",
          "code":"REVIEW",
          "uri":"https://form.sjtu.edu.cn/x",
          "assignTime":1773540999,
          "status":2,
          "process":{
            "name":"匿名通知项",
            "status":"doing",
            "app":{"name":"通知服务平台","system":"通知服务平台","visible":false},
            "owner":{"account":"system-bot"}
          }
        }
      ]
    }"#;
    let env: TodoEnvelope = serde_json::from_str(fixture).unwrap();
    assert_eq!(env.total, 1);
    let it = &env.entities[0];
    assert_eq!(it.code.as_deref(), Some("REVIEW"));

    let p = it.process.as_ref().unwrap();
    assert!(p.entry.is_none());
    assert!(p.id.is_none());
    assert!(p.create.is_none());
    assert!(p.milestone.is_none());

    let app = p.app.as_ref().unwrap();
    assert!(app.code.is_none());
    assert_eq!(app.system.as_deref(), Some("通知服务平台"));
    assert_eq!(app.visible, Some(false));

    let owner = p.owner.as_ref().unwrap();
    assert_eq!(owner.account.as_deref(), Some("system-bot"));
    assert!(owner.name.is_none());
    assert!(owner.id.is_none());
}

#[test]
fn parse_empty_todo_list_tolerates() {
    let fixture = r#"{"errno":0,"error":"","total":0,"entities":[]}"#;
    let env: TodoEnvelope = serde_json::from_str(fixture).unwrap();
    assert_eq!(env.total, 0);
    assert_eq!(env.entities.len(), 0);
}

#[test]
fn code_add_distinguishes_my_applications() {
    // 用真实分流逻辑（同 cmd_pending 的过滤口径）做契约固化。
    let items: Vec<TodoItem> = serde_json::from_str(
        r#"[
          {"id":"a","code":"ADD","name":"我的"},
          {"id":"b","code":"REVIEW","name":"等审"},
          {"id":"c","code":"ADD","name":"也是我的"},
          {"id":"d","code":null,"name":"无 code"}
        ]"#,
    )
    .unwrap();
    let (mine, others): (Vec<_>, Vec<_>) = items
        .into_iter()
        .partition(|i| i.code.as_deref() == Some("ADD"));
    assert_eq!(mine.len(), 2);
    assert_eq!(others.len(), 2);
}
