//! apps::canvas 响应解析单测 + throttle 单测。
//!
//! fixture 精简自 tasks/s3c-canvas-planner.md §2 真实抓包，字段语义字字对齐。
//! HTTP 路径（URL / header / 401 分支）靠真机 CP-C1/C2 验证。

use super::models::{PlannerItem, Profile, Submissions, UserProfile, UserSelf};
use super::throttle::{Throttle, MIN_INTERVAL};

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
fn parse_user_self_basic() {
    // 真实抓包（脱敏后）：id/name/created_at/short_name/locale=null/effective_locale/permissions
    let fixture = r#"{
      "id":"999999",
      "name":"张三",
      "short_name":"张三",
      "sortable_name":"999999999999-张三",
      "avatar_url":"https://oc.sjtu.edu.cn/images/messages/avatar-50.png",
      "locale":null,
      "effective_locale":"zh-Hans",
      "permissions":{"can_update_name":false,"can_update_avatar":true}
    }"#;
    let u: UserSelf = serde_json::from_str(fixture).unwrap();
    assert_eq!(u.id, "999999");
    assert_eq!(u.name, "张三");
    assert_eq!(u.effective_locale.as_deref(), Some("zh-Hans"));
    assert!(u.locale.is_none());
}

#[test]
fn parse_user_profile_with_calendar() {
    let fixture = r#"{
      "id":"999999","name":"张三","short_name":"张三",
      "sortable_name":"999999999999-张三",
      "primary_email":"zhangsan@sjtu.edu.cn",
      "login_id":"zhangsan",
      "integration_id":null,
      "time_zone":"Asia/Shanghai",
      "locale":null,"effective_locale":"zh-Hans",
      "calendar":{"ics":"https://oc.sjtu.edu.cn/feeds/calendars/user_XYZ.ics"},
      "lti_user_id":"uuid-xxx","k5_user":false
    }"#;
    let p: UserProfile = serde_json::from_str(fixture).unwrap();
    assert_eq!(p.login_id.as_deref(), Some("zhangsan"));
    assert_eq!(p.time_zone.as_deref(), Some("Asia/Shanghai"));
    assert_eq!(
        p.calendar.unwrap().ics.as_deref(),
        Some("https://oc.sjtu.edu.cn/feeds/calendars/user_XYZ.ics")
    );
}

#[test]
fn profile_merge_prefers_profile_over_user() {
    let user = UserSelf {
        id: "u1".into(),
        name: "u-name".into(),
        short_name: Some("u-short".into()),
        effective_locale: Some("en".into()),
        ..Default::default()
    };
    let profile = UserProfile {
        id: "p1".into(),
        name: "p-name".into(),
        short_name: None,
        login_id: Some("jaccount".into()),
        time_zone: Some("Asia/Shanghai".into()),
        effective_locale: Some("zh-Hans".into()),
        ..Default::default()
    };
    let merged: Profile = Profile::merge(user, profile);
    // profile 非空字段胜出
    assert_eq!(merged.id, "p1");
    assert_eq!(merged.name, "p-name");
    // profile.short_name=None 时回落 user
    assert_eq!(merged.short_name.as_deref(), Some("u-short"));
    assert_eq!(merged.login_id.as_deref(), Some("jaccount"));
    assert_eq!(merged.time_zone.as_deref(), Some("Asia/Shanghai"));
    // locale 优先 profile
    assert_eq!(merged.effective_locale.as_deref(), Some("zh-Hans"));
}

#[test]
fn parse_planner_items_assignment_and_note() {
    // 精简自真实抓包 (start_date=2026-04-23T16:00Z)，混合 assignment + planner_note 两种。
    let fixture = r#"[
      {
        "context_type":"Course","course_id":"88169","plannable_id":"405484",
        "planner_override":null,"plannable_type":"assignment","new_activity":false,
        "submissions":{"submitted":false,"excused":false,"graded":false,"posted_at":null,
                       "late":false,"missing":false,"needs_grading":false,
                       "has_feedback":false,"redo_request":false},
        "plannable_date":"2026-04-25T15:59:59Z",
        "plannable":{"id":"405484","title":"提交演讲稿","created_at":"2026-04-22T11:45:50Z",
                     "updated_at":"2026-04-22T11:45:51Z","points_possible":75.0,
                     "due_at":"2026-04-25T15:59:59Z"},
        "html_url":"/courses/88169/assignments/405484",
        "context_name":"日语演讲比赛（3）",
        "context_image":null
      },
      {
        "context_type":"User","plannable_id":"note-1",
        "plannable_type":"planner_note","new_activity":false,
        "plannable_date":"2026-04-26T08:00:00Z",
        "plannable":{"id":"note-1","title":"自建待办","points_possible":null,
                     "due_at":null,"created_at":"2026-04-20T00:00:00Z",
                     "updated_at":"2026-04-20T00:00:00Z"},
        "html_url":"/planner_notes/note-1",
        "context_name":"我的待办"
      }
    ]"#;
    let items: Vec<PlannerItem> = serde_json::from_str(fixture).unwrap();
    assert_eq!(items.len(), 2);

    let a = &items[0];
    assert_eq!(a.plannable_type.as_deref(), Some("assignment"));
    assert_eq!(a.course_id.as_deref(), Some("88169"));
    assert_eq!(a.plannable_date.as_deref(), Some("2026-04-25T15:59:59Z"));
    let p = a.plannable.as_ref().unwrap();
    assert_eq!(p.title.as_deref(), Some("提交演讲稿"));
    assert_eq!(p.points_possible, Some(75.0));
    assert_eq!(a.context_name.as_deref(), Some("日语演讲比赛（3）"));
    let s = a.submissions.as_ref().unwrap();
    assert!(!s.submitted);
    assert!(!s.missing);

    // 第二条：planner_note —— submissions 字段缺失，plannable.due_at 为 null
    let n = &items[1];
    assert_eq!(n.plannable_type.as_deref(), Some("planner_note"));
    assert!(n.submissions.is_none());
    assert!(n.plannable.as_ref().unwrap().due_at.is_none());
}

#[test]
fn submissions_is_outstanding_semantics() {
    // 未交 + 未免交 → 未完成
    let s1 = Submissions {
        submitted: false,
        excused: false,
        ..Default::default()
    };
    assert!(Submissions::is_outstanding(Some(&s1)));
    // 已交 → 已完成
    let s2 = Submissions {
        submitted: true,
        ..Default::default()
    };
    assert!(!Submissions::is_outstanding(Some(&s2)));
    // 免交 → 已完成
    let s3 = Submissions {
        submitted: false,
        excused: true,
        ..Default::default()
    };
    assert!(!Submissions::is_outstanding(Some(&s3)));
    // None → 视为未完成（planner_note）
    assert!(Submissions::is_outstanding(None));
}
