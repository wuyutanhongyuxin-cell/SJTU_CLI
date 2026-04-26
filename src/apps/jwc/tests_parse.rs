//! ZF 响应反序列化单元测试 —— 不打真服务器，只跑 fixture JSON。
//!
//! 覆盖 §2.1 N305005 的 8 大坑里 fixture 可验的部分：
//! - `totalResult` 是字符串（"52"）能被 `serde_json::Value` 兜住
//! - `cj` mixed types："86" / "P" / "" / 字母 都能进 `Option<String>` 不 panic
//! - `xf` / `jd` / `xfjd` 全 String，不被强转
//! - 空 items 列表（当前学期成绩未出）能正常 deserialize 为空 Vec

use super::models::{Grade, JwcPage};

#[test]
fn parse_grade_envelope_string_total_result() {
    let body = r#"{
        "currentPage": 1,
        "pageSize": 50,
        "totalResult": "52",
        "totalPage": 2,
        "items": []
    }"#;
    let p: JwcPage<Grade> = serde_json::from_str(body).unwrap();
    assert_eq!(p.total_result.as_ref().unwrap().as_str(), Some("52"));
    assert!(p.items.is_empty());
}

#[test]
fn parse_grade_envelope_int_total_result() {
    let body = r#"{
        "currentPage": 1,
        "totalResult": 0,
        "items": []
    }"#;
    let p: JwcPage<Grade> = serde_json::from_str(body).unwrap();
    assert!(p.total_result.as_ref().unwrap().is_number());
    assert!(p.items.is_empty());
}

#[test]
fn parse_grade_item_mixed_cj_types() {
    let body = r#"{
        "items": [
            {"kch": "FL1405", "kcmc": "英语", "xf": "2.0", "cj": "86", "jd": "3.7"},
            {"kch": "PE001", "kcmc": "体育", "xf": "1.0", "cj": "P", "jd": ""},
            {"kch": "MS101", "kcmc": "军训", "xf": "0", "cj": "通过"},
            {"kch": "TM001", "kcmc": "考核课", "xf": "1.0", "cj": "良"}
        ]
    }"#;
    let p: JwcPage<Grade> = serde_json::from_str(body).unwrap();
    assert_eq!(p.items.len(), 4);
    let cjs: Vec<_> = p.items.iter().map(|g| g.cj.as_deref()).collect();
    assert_eq!(cjs, vec![Some("86"), Some("P"), Some("通过"), Some("良")]);
    assert_eq!(p.items[0].xf.as_deref(), Some("2.0"));
    assert_eq!(p.items[0].jd.as_deref(), Some("3.7"));
    assert_eq!(p.items[1].jd.as_deref(), Some(""));
}

#[test]
fn parse_grade_item_resilient_to_missing_fields() {
    // 真实场景：英文名 / 教师 / 班名 / 成绩录入时间任意缺失
    let body = r#"{ "items": [ { "kch": "X1", "kcmc": "短缺字段课" } ] }"#;
    let p: JwcPage<Grade> = serde_json::from_str(body).unwrap();
    assert_eq!(p.items[0].kch.as_deref(), Some("X1"));
    assert!(p.items[0].kcywmc.is_none());
    assert!(p.items[0].cjbdsj.is_none());
}
