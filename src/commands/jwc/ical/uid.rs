//! RFC 5545 UID 生成：FNV-1a 64-bit 手卷 hash + `@sjtu-cli` domain。
//!
//! 零依赖（OQ2 决策）。FNV-1a 算法顺序：`(h ^ b).wrapping_mul(PRIME)`。
//! testvec：`fnv1a_64("foobar") == "85944171f73967e8"`

/// FNV-1a 64-bit hash → 16 字符 lowercase hex。
// TODO(T5-T6): cmd_calendar 落地后删 allow
#[allow(dead_code)]
pub fn fnv1a_64(s: &str) -> String {
    const OFFSET: u64 = 14_695_981_039_346_656_037;
    const PRIME: u64 = 1_099_511_628_211;
    let hash = s
        .bytes()
        .fold(OFFSET, |h, b| (h ^ b as u64).wrapping_mul(PRIME));
    format!("{hash:016x}")
}

/// UID = `fnv1a_64(key)@sjtu-cli`，符合 RFC 5545 unique-id。
// TODO(T5-T6): cmd_calendar 落地后删 allow
#[allow(dead_code)]
pub fn make_uid(key: &str) -> String {
    format!("{}@sjtu-cli", fnv1a_64(key))
}
