//! apps::canvas_video 解析单测 + throttle 间隔单测 + mockito findVodVideoList 全链路单测。
//!
//! fixture 精简自 tasks/canvas_video_investigation.md §4-§5，字段语义字字对齐。
//! 真机端到端（含 LTI launch 弹 chrome）靠 CP-V2 真机走，不在本文件覆盖。

use std::sync::Arc;

use reqwest::cookie::Jar;
use reqwest::redirect::Policy;
use reqwest::Client as HttpClient;

use super::http::post_json;
use super::models::{ApiEnvelope, FindListResponse, GetTokenResponse};
use super::throttle::{Throttle, MIN_INTERVAL};

/// mockito 专用 HTTP client：手动 `no_proxy()`（系统代理会把 127.0.0.1 mock 服务地址
/// 截走并改写为 503，参考 jwbmessage::tests_write::bare_client 的 lesson）。
fn bare_client() -> HttpClient {
    HttpClient::builder()
        .cookie_provider(Arc::new(Jar::default()))
        .redirect(Policy::none())
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap()
}

#[tokio::test]
async fn throttle_enforces_minimum_interval() {
    let t = Throttle::new();
    t.wait().await; // 首次立即返回
    let t0 = std::time::Instant::now();
    t.wait().await;
    let elapsed = t0.elapsed();
    // Windows 下 tokio sleep 精度 ±15ms，下界放宽。
    assert!(
        elapsed >= MIN_INTERVAL.saturating_sub(std::time::Duration::from_millis(15)),
        "节流间隔过短：{elapsed:?}，期望 >= {MIN_INTERVAL:?}"
    );
}

#[test]
fn url_encode_component_matches_browser() {
    use super::auth::url_encode_component;
    // 浏览器 encodeURIComponent 行为：保留 -_.~ 与字母数字，其他全转 %XX。
    assert_eq!(url_encode_component("abc-_.~"), "abc-_.~");
    // courId 典型字符 / + = → 应全部转义
    assert_eq!(url_encode_component("a/b+c=d"), "a%2Fb%2Bc%3Dd");
    // 中文（UTF-8 多字节）
    assert_eq!(url_encode_component("课"), "%E8%AF%BE");
}

#[test]
fn parse_get_token_response_minimal() {
    // 只保留 CLI 关心的字段，userCode/userName/jwt_token 字段故意不在 fixture
    // —— 即使后端真返了，我们的 TokenData struct 也不会反序列化出来（PII 不入 struct）。
    let fixture = r#"{
      "code":"0","success":true,"status":200,"message":null,"timestamp":1746000000000,
      "data":{
        "token":"eyJhbGciOiJIUzUxMiIsInppcCI6IkdaSVAifQ.STUB.SIG",
        "params":{
          "courId":"abc/+def==",
          "ltiCourseId":"0123456789abcdef0123456789abcdef",
          "courseName":"日语语言学专题研讨2",
          "clientId":"10000000000025"
        },
        "ivsUserId":"99999",
        "roles":["StudentEnrollment"]
      }
    }"#;
    let env: GetTokenResponse = serde_json::from_str(fixture).unwrap();
    assert!(env.is_business_ok());
    let data = env.data.unwrap();
    assert_eq!(
        data.token.as_deref(),
        Some("eyJhbGciOiJIUzUxMiIsInppcCI6IkdaSVAifQ.STUB.SIG")
    );
    let params = data.params.unwrap();
    assert_eq!(params.cour_id.as_deref(), Some("abc/+def=="));
    assert_eq!(
        params.lti_course_id.as_deref(),
        Some("0123456789abcdef0123456789abcdef")
    );
}

#[test]
fn business_ok_requires_code_zero_and_success_true() {
    // 只 code=0 不够
    let env: ApiEnvelope<()> = serde_json::from_str(r#"{"code":"0","success":false}"#).unwrap();
    assert!(!env.is_business_ok());
    // 只 success=true 不够
    let env: ApiEnvelope<()> = serde_json::from_str(r#"{"code":"1","success":true}"#).unwrap();
    assert!(!env.is_business_ok());
    // 都有才算 OK
    let env: ApiEnvelope<()> = serde_json::from_str(r#"{"code":"0","success":true}"#).unwrap();
    assert!(env.is_business_ok());
}

#[test]
fn parse_find_vod_video_list_records() {
    // 精简自调研 §5：核心字段 + PII 字段（userName 教师姓名）+ 系统字段。
    let fixture = r#"{
      "code":"0","success":true,"status":200,"message":null,"timestamp":1746000000001,
      "data":{
        "total":2,"size":2000,"current":1,"pages":1,
        "records":[
          {
            "videoId":"VIDEOID_BASE64_W==",
            "videoName":"日语语言学专题研讨2(第01讲)",
            "courseBeginTime":"2026-03-06 08:00:00",
            "courseEndTime":"2026-03-06 09:40:00",
            "classroomName":"上院210",
            "userName":"老师姓名",
            "courId":1234567,
            "videAuditStatus":3,
            "playCount":12,
            "playTime":3600,
            "playTimes":2,
            "playAverage":0.5,
            "subjId":99,
            "uebung":false,
            "partClose":false,
            "videSource":1,
            "inSchoolVodStatus":1,
            "teclId":42
          },
          {
            "videoId":"VIDEOID_2==",
            "videoName":"日语语言学专题研讨2(第02讲)",
            "courseBeginTime":"2026-03-13 08:00:00",
            "courseEndTime":"2026-03-13 09:40:00",
            "classroomName":"上院210",
            "userName":"老师姓名",
            "videAuditStatus":1
          }
        ]
      }
    }"#;
    let env: FindListResponse = serde_json::from_str(fixture).unwrap();
    assert!(env.is_business_ok());
    let page = env.data.unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.records.len(), 2);
    let v0 = &page.records[0];
    assert_eq!(v0.video_id, "VIDEOID_BASE64_W==");
    assert!(v0.video_name.contains("第01讲"));
    assert_eq!(v0.user_name.as_deref(), Some("老师姓名"));
    assert_eq!(v0.classroom_name.as_deref(), Some("上院210"));
    assert_eq!(v0.cour_id, Some(1234567));
    assert_eq!(v0.vide_audit_status, Some(3));
    // 第二条：videAuditStatus=1（未审核）→ CLI 上层应过滤掉
    let v1 = &page.records[1];
    assert_eq!(v1.vide_audit_status, Some(1));
    // 字段缺失（如 courId / playCount）应被 #[serde(default)] 兜底
    assert_eq!(v1.cour_id, None);
}

#[tokio::test]
async fn post_json_returns_parsed_response_via_mockito() {
    let mut server = mockito::Server::new_async().await;
    let body = r#"{"code":"0","success":true,"status":200,"data":{"total":0,"records":[]}}"#;
    let _m = server
        .mock(
            "POST",
            "/jy-application-canvas-sjtu/directOnDemandPlay/findVodVideoList",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create_async()
        .await;

    let http = bare_client();
    let throttle = Arc::new(Throttle::new());
    let url = format!(
        "{}/jy-application-canvas-sjtu/directOnDemandPlay/findVodVideoList",
        server.url()
    );
    let resp: FindListResponse = post_json(
        &http,
        &throttle,
        &url,
        "stub-token",
        r#"{"canvasCourseId":"x"}"#,
        "/findVodVideoList",
    )
    .await
    .unwrap();
    assert!(resp.is_business_ok());
    assert_eq!(resp.data.unwrap().total, 0);
}

#[tokio::test]
async fn post_json_propagates_non_2xx_with_snippet() {
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/x")
        .with_status(500)
        .with_body("internal err snippet")
        .create_async()
        .await;
    let http = bare_client();
    let throttle = Arc::new(Throttle::new());
    let url = format!("{}/x", server.url());
    let err = post_json::<serde_json::Value>(&http, &throttle, &url, "tok", "{}", "/x")
        .await
        .unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("status=500"), "应带 status：{msg}");
    assert!(msg.contains("internal err snippet"), "应带 snippet：{msg}");
}
