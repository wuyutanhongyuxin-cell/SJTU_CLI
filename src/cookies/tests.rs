//! cookies 模块的纯函数单元测试。文件 IO（load/save）不走这里，
//! 交给后续的 tempfile 集成测试覆盖。

use super::io::sub_session_path;
use super::*;

#[test]
fn sub_session_name_rejects_path_traversal() {
    assert!(sub_session_path("").is_err());
    assert!(sub_session_path("..").is_err());
    assert!(sub_session_path("a/b").is_err());
    assert!(sub_session_path("a\\b").is_err());
    assert!(sub_session_path("with space").is_err());
    assert!(sub_session_path("ok_name").is_ok());
    assert!(sub_session_path("jwc").is_ok());
}

/// RFC 6265 §5.3 的三元组唯一键：同 name 同 domain 不同 path 必须各占一行，
/// 不能被 `redacted()` 的 HashMap 静默覆盖。
#[test]
fn redacted_separates_same_name_diff_path() {
    let mk = |path: &str, val: &str| Cookie {
        name: "JSESSIONID".into(),
        value: val.into(),
        domain: Some("i.sjtu.edu.cn".into()),
        path: Some(path.into()),
        expires: None,
    };
    let s = Session::new(vec![
        mk("/", "aaaaaaaaaa_root"),
        mk("/xtgl", "bbbbbbbbbb_xtgl"),
    ]);
    let r = s.redacted();
    assert_eq!(r.len(), 2, "同 name 同 domain 不同 path 不应被覆盖");
    assert!(r.contains_key("JSESSIONID@i.sjtu.edu.cn,/"));
    assert!(r.contains_key("JSESSIONID@i.sjtu.edu.cn,/xtgl"));
}

/// domain/path 缺失时 key 退化成 `-`，不能 panic。
#[test]
fn redacted_handles_missing_domain_and_path() {
    let c = Cookie {
        name: "X".into(),
        value: "12345678abc".into(),
        domain: None,
        path: None,
        expires: None,
    };
    let r = Session::new(vec![c]).redacted();
    assert_eq!(r.len(), 1);
    assert!(r.contains_key("X@-,-"));
    assert_eq!(r["X@-,-"], "12345678***");
}
