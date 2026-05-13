//! ZF 响应反序列化单元测试 —— 不打真服务器，只跑 fixture JSON。
//!
//! 覆盖 §2.1..§2.4 的关键反序列化路径：
//! - §2.1 N305005：mixed cj 类型 / `totalResult` 字符串 / 字段缺失韧性
//! - §2.2 N2151 ：专属 envelope（kbList + xqjmcMap）+ 身份字段 (xsxx) 静默丢弃
//! - §2.3 N309131：step1 裸 JSON 字符串 / step2 envelope 含 gpa / gpapm 字段
//! - §2.4 N358105：标准分页 + kssj 复合时间字符串保留原样

use super::models::{parse_rank, Exam, Gpa, Grade, JwcPage, KbItem, RankPair, Schedule};

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

#[test]
fn parse_schedule_envelope_drops_identity_xsxx() {
    // §2.2 fixture：xsxx 含身份字段（XH/XM/YWXM/NJDM_ID/ZYMC/BJMC）—— **必须静默丢弃**
    // kbList 课程 + xqjmcMap 周几映射保留。
    let body = r#"{
        "qsxqj": "1",
        "xsxx": {"XH":"5xxxxxxxxx","XM":"张三","NJDM_ID":"2023","ZYMC":"日语","BJMC":"日语2301"},
        "sjkList": [],
        "sjfwkg": true,
        "rqazcList": [],
        "xskbsfxstkzt": "0",
        "xqjmcMap": {"1":"星期一","2":"星期二","7":"星期日"},
        "kbList": [
            {"xnm":"2025","xqm":"3","kch":"FL1405","kcmc":"日语精读","xf":"3.0",
             "kcxz":"必修","jxbmc":"(2025-2026-1)-FL1405-01","xqj":"1","xqjmc":"星期一",
             "jc":"3-4节","jcs":"03-04","jcor":"3-4","zcd":"1-16周","cdmc":"上院 412",
             "xm":"佐藤","skfsmc":"中文"}
        ]
    }"#;
    let s: Schedule = serde_json::from_str(body).unwrap();
    assert_eq!(s.kb_list.len(), 1);
    let k: &KbItem = &s.kb_list[0];
    assert_eq!(k.kch.as_deref(), Some("FL1405"));
    assert_eq!(k.zcd.as_deref(), Some("1-16周"));
    assert_eq!(k.xqj.as_deref(), Some("1"));
    assert_eq!(k.cdmc.as_deref(), Some("上院 412"));
    // xqjmcMap 原样保留（serde_json::Value）
    assert_eq!(
        s.xqjmc_map.get("1").and_then(|v| v.as_str()),
        Some("星期一")
    );
    // Schedule 没有 xsxx 字段 —— xsxx 被 serde 静默丢弃 ✓（编译期保证）
}

#[test]
fn parse_gpa_step1_bare_string() {
    // §2.3 step 1 返裸 JSON 字符串 `"统计成功！"`，不是对象。
    let body = r#""统计成功！""#;
    let s: String = serde_json::from_str(body).unwrap();
    assert!(s.contains("统计成功"));
}

#[test]
fn parse_gpa_envelope_step2_items() {
    // §2.3 step 2 标准分页 envelope，items[0] 含 gpa / 排名 / 学积分等。
    let body = r#"{
        "currentPage":1,"pageSize":50,"totalResult":"1","totalPage":1,
        "items":[{
            "gpa":"3.85","gpapm":"3/120","xjf":"88.5","xjfpm":"5/120",
            "zf":"1234","ms":"14","bjgmc":"0","bjgms":"0",
            "zxf":"40.0","hdxf":"40.0","bjgxf":"0",
            "tgl":"100%","kcfw":"hxkc","czsj":"2026-04-30 12:34:56"
        }]
    }"#;
    let p: JwcPage<Gpa> = serde_json::from_str(body).unwrap();
    assert_eq!(p.items.len(), 1);
    let g = &p.items[0];
    assert_eq!(g.gpa.as_deref(), Some("3.85"));
    assert_eq!(g.gpapm.as_deref(), Some("3/120"));
    assert_eq!(g.tgl.as_deref(), Some("100%"));
    assert_eq!(g.kcfw.as_deref(), Some("hxkc"));
}

#[test]
fn parse_exams_envelope_compound_kssj() {
    // §2.4 fixture：kssj 是复合时间字符串 `YYYY-MM-DD(HH:MM-HH:MM)`，
    // CLI 不在 model 层 parse —— 原样保留。空 items 也不报错。
    let body_empty = r#"{"totalResult":0,"items":[]}"#;
    let p_empty: JwcPage<Exam> = serde_json::from_str(body_empty).unwrap();
    assert!(p_empty.items.is_empty());

    let body = r#"{
        "currentPage":1,"totalResult":"2","items":[
            {"xnm":"2025","xqmmc":"1","ksmc":"2025-2026-1期末考试",
             "kssj":"2026-01-08(10:30-12:30)","kch":"FL1405","kcmc":"日语精读",
             "jxbmc":"(2025-2026-1)-FL1405-01","xf":"3.0","khfs":"考试","ksfs":"笔试",
             "cdmc":"上院 412","cdbh":"SY412","cdxqmc":"闵行","kkxy":"外国语学院",
             "jsxx":"5xxxxxxx/张老师","sjbh":"学校统一-2025-2026-1期末考试-FL1405",
             "cxbj":"否","pycc":"本科"},
            {"xnm":"2025","ksmc":"2025-2026-1期末考试","kssj":"2026-01-09(14:00-16:00)",
             "kch":"MA101","kcmc":"高数","cdmc":"东上院 102","cxbj":"否"}
        ]
    }"#;
    let p: JwcPage<Exam> = serde_json::from_str(body).unwrap();
    assert_eq!(p.items.len(), 2);
    assert_eq!(p.items[0].kssj.as_deref(), Some("2026-01-08(10:30-12:30)"));
    assert_eq!(p.items[0].cdmc.as_deref(), Some("上院 412"));
    assert_eq!(p.items[0].jsxx.as_deref(), Some("5xxxxxxx/张老师"));
    // 第二条字段稀疏也不 panic
    assert_eq!(p.items[1].kch.as_deref(), Some("MA101"));
    assert!(p.items[1].cdbh.is_none());
}

#[test]
fn parse_rank_normal_3_over_120() {
    let r = parse_rank("3/120").unwrap();
    assert_eq!(r.rank, 3);
    assert_eq!(r.total, 120);
    let p = r.percentile.unwrap();
    assert!((p - 2.5).abs() < 1e-6, "percentile={p}");
}

#[test]
fn parse_rank_total_zero_keeps_pair_drops_percentile() {
    let r = parse_rank("0/0").unwrap();
    assert_eq!(r.rank, 0);
    assert_eq!(r.total, 0);
    assert!(r.percentile.is_none(), "total=0 时 percentile 必须 None");
}

#[test]
fn parse_rank_empty_returns_none() {
    assert!(parse_rank("").is_none());
    assert!(parse_rank("   ").is_none());
}

#[test]
fn parse_rank_no_slash_returns_none() {
    assert!(parse_rank("3").is_none());
    assert!(parse_rank("abc").is_none());
    assert!(parse_rank("3-120").is_none());
}

#[test]
fn parse_rank_rank_greater_than_total_drops_percentile() {
    let r = parse_rank("200/120").unwrap();
    assert_eq!(r.rank, 200);
    assert_eq!(r.total, 120);
    assert!(r.percentile.is_none(), "rank>total 时 percentile 必须 None");
}

#[test]
fn parse_rank_tolerates_whitespace() {
    let r = parse_rank("  3 / 120  ").unwrap();
    assert_eq!(r.rank, 3);
    assert_eq!(r.total, 120);
    assert_eq!(r.percentile, Some(2.5));
}

#[test]
fn gpa_fill_parsed_populates_both_from_strings() {
    let mut g = Gpa {
        gpapm: Some("3/120".into()),
        xjfpm: Some("5/120".into()),
        ..Default::default()
    };
    g.fill_parsed();
    let gpa_pair: &RankPair = g.gpapm_parsed.as_ref().unwrap();
    let xjf_pair: &RankPair = g.xjfpm_parsed.as_ref().unwrap();
    assert_eq!(gpa_pair.rank, 3);
    assert_eq!(xjf_pair.rank, 5);
}

#[test]
fn gpa_fill_parsed_keeps_none_when_strings_invalid() {
    let mut g = Gpa {
        gpapm: Some("".into()),
        xjfpm: None,
        ..Default::default()
    };
    g.fill_parsed();
    assert!(g.gpapm_parsed.is_none());
    assert!(g.xjfpm_parsed.is_none());
}

// === T2 集成测试：N309131 两阶段串联 + fixture 反序列化 ===

const N309131_STEP1_FIXTURE: &str =
    include_str!("../../../tests/fixtures/jwc/n309131_step1_success.txt");

const N309131_STEP2_FIXTURE: &str =
    include_str!("../../../tests/fixtures/jwc/n309131_step2_hxkc_njzy.json");

#[test]
fn fixture_step1_parses_as_success_string() {
    let s: String = serde_json::from_str(N309131_STEP1_FIXTURE).unwrap();
    assert!(s.contains("统计成功"), "fixture 必须含'统计成功'：{s}");
}

#[test]
fn fixture_step2_parses_envelope_with_required_fields() {
    let p: JwcPage<Gpa> = serde_json::from_str(N309131_STEP2_FIXTURE).unwrap();
    assert!(!p.items.is_empty(), "fixture step2 必须含至少 1 条 item");
    let g = &p.items[0];
    assert!(g.gpa.is_some(), "gpa 字段必须保留");
    assert!(g.gpapm.is_some(), "gpapm 字段必须保留");
    assert!(
        g.kcfw.is_some(),
        "kcfw 字段必须保留（server 返中文 '核心课程' 而非 hxkc）"
    );
}

#[test]
fn fixture_step2_round_trip_with_fill_parsed() {
    let mut p: JwcPage<Gpa> = serde_json::from_str(N309131_STEP2_FIXTURE).unwrap();
    for g in &mut p.items {
        g.fill_parsed();
    }
    let g = &p.items[0];
    // 双轨断言：原字符串保留 + parsed 填充
    assert!(g.gpapm.is_some());
    assert!(
        g.gpapm_parsed.is_some(),
        "fill_parsed 后 gpapm_parsed 必须有值"
    );
    let rp = g.gpapm_parsed.as_ref().unwrap();
    assert!(rp.total > 0, "fixture 排名 total 必须 > 0");
}
