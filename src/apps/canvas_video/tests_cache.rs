//! cache 行为测：过期 / 坏数据 / 路径校验等。
//! 从 tests_parse.rs 拆出来防超 300 行硬限。

use super::cache;
use chrono::Utc;

/// V5.A：cache_expired_returns_none — saved_at 早于 TTL 之外，load 返 None 不 panic。
#[test]
fn cache_expired_returns_none() {
    let path = std::env::temp_dir().join("sjtu_cli_v5a_expired.json");
    let _ = std::fs::remove_file(&path);

    // 手构造一个 31 分钟前 saved 的缓存（TTL 30min），写到磁盘。
    let stale = serde_json::json!({
        "course_id": 88168u64,
        "lti_tool_id": 8329u64,
        // 31 分钟前
        "saved_at": (Utc::now() - chrono::Duration::seconds(31 * 60)).to_rfc3339(),
        "ttl_secs": 1800u64,
        "token": "STALE",
        "cour_id": "X",
        "lti_course_id": "Y",
        "session_cookies": [],
    });
    std::fs::write(&path, serde_json::to_string_pretty(&stale).unwrap()).unwrap();

    let loaded = cache::load_from_path(&path, 88168, 8329);
    assert!(loaded.is_none(), "TTL 过期应返 None，但拿到了 Some");

    let _ = std::fs::remove_file(&path);
}
