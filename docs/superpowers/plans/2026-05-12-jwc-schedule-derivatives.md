# jwc 课表衍生命令（today / week / next）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在已实装 N2151 学年学期课表之上，新增 N2154 周次端点 + 三个衍生 CLI 命令（`sjtu jwc today` / `week` / `next`）+ 节次时刻映射 + 可选 `--grid` 表格输出。

**Architecture:** 纯增量 / 0 改造。N2151 路径完全不动；N2154 是平行通道。`schedule_by_week(xnm, xqm, zs)` 返回 N2154 envelope（含 `rqazcList` + `oldzc/oldjc` bitmask），CLI 层用 bitmask 精确过滤本周课，并 join `period_clock` 表得到节次时刻。`infer_current_week` 反推今天周次（zs=1 → rqazc_list[0].rq → 今日差），结果落 `~/.cache/sjtu-cli/jwc_week_cache.json`，TTL 24h（显式 xnm/xqm）/ 1h（"__current__"）。

**Tech Stack:** Rust 2021 / clap 4 derive / reqwest (cookies+rustls+gzip) / tokio (multi-thread) / chrono (NaiveDate/NaiveTime) / directories (cache_dir) / **comfy-table = "~7.1"** (新增；锁 7.1.x 因 7.2 MSRV 1.85 超项目 rust-version 1.75) / mockito (单测).

**Spec:** `docs/superpowers/specs/2026-05-12-jwc-schedule-derivatives-design.md`

---

## Task 顺序总览

| # | 类型 | 名称 | Subagent 可跑？ |
|---|---|---|---|
| T0 | main | chrome-devtools 调研 ZF 节次字典端点 + 真机 N2154 抓 fixture | ❌ 需 chrome 句柄 + SJTU 登录 |
| T1 | sub | Cargo.toml 加 `comfy-table = "~7.1"` | ✅ |
| T2 | sub | `src/config.rs` 加 `cache_dir()` + `jwc_week_cache_path()` + `ensure_cache_dir()` | ✅ |
| T3 | sub | `src/apps/jwc/models/schedule.rs` 扩 `KbItem` + `RqAzc` + `Schedule.rqazc_list` | ✅ |
| T4 | sub | `src/apps/jwc/period_clock.rs` 新建（fallback 表 + lookup + bitmask helpers） | ✅ |
| T5 | sub | `src/apps/jwc/api/schedule.rs` 加 `schedule_by_week` | ✅ |
| T6 | sub | `src/apps/jwc/api/schedule.rs` 加 `infer_current_week` + cache I/O | ✅ |
| T7 | sub | 拆 `src/cli/jwc.rs` → `src/cli/jwc/{mod,schedule_cli}.rs` | ✅ |
| T8 | sub | `src/commands/jwc/data.rs` 加 `TodayData` / `WeekData` / `NextData` | ✅ |
| T9 | sub | `src/commands/jwc/schedule_handlers.rs` 新建 (`cmd_today` / `cmd_week` / `cmd_next`) | ✅ |
| T10 | sub | `src/output/grid.rs` 新建（comfy-table 渲染） | ✅ |
| T11 | sub | 集成测试（mockito + T0 抓的 fixture） | ✅ |
| T12 | main | 真机 smoke 测 today / week / next | ❌ 需 SJTU 登录 |
| T13 | main | README / lessons.md / todo.md 收尾 | ❌ 跨文档判断 |

**Open Questions 收口**（plan 阶段决策，subagent 直接执行）：

- **OQ1 (throttle 并发)**：`src/apps/jwc/throttle.rs` 是进程级 `Mutex<Instant>` + 500ms 间隔，并发 spawn 也会被排队。**决策**：MVP 串行调用 N2154，`cmd_next --within 31` 用 `for zs in cw..cw+weeks_to_fetch { ... }` 顺序拉。预期 ≤ 3s 总耗时。
- **OQ2 (xnm/xqm 空时 cache key)**：`(xnm, xqm)` 任一为 None → cache key 用 `"__current__"`，TTL 缩到 1h；显式时用 `"{xnm}-{xqm}"`，TTL 24h。
- **OQ3 (窄终端 grid 退化)**：用 `comfy_table::ContentArrangement::Dynamic`，由 comfy-table 自动检测终端宽度并 wrap 内容。不引入 `terminal_size` crate。

---

## Task T0：ZF 节次字典调研 + N2154 fixture 抓取

> **必须主对话亲跑**：subagent 没有 chrome-devtools 句柄、没有 SJTU JAccount session。

**Files:**
- Create: `tests/fixtures/jwc/n2154_week_zs1.json`
- Create: `tests/fixtures/jwc/n2154_week_zs14.json`
- Update: `tasks/isjtu_investigation.md`（§2.7 末尾补节次时刻表）
- Update: `src/apps/jwc/period_clock.rs` 的 `DEFAULT_TABLE` 常量（在 T4 后再回填）

- [ ] **Step 1：chrome-devtools 调研 ZF 字典端点**

用 mcp__chrome-devtools 工具（**只读 JS**，遵守 CLAUDE.md i.sjtu 红线）：

1. `take_snapshot` 当前 `https://i.sjtu.edu.cn` 页面（确保已 SJTU 登录）
2. `evaluate_script` 注入只读 fetch 调研字典端点：
   ```javascript
   // 候选 1：ZF 公共字典模式
   const r = await fetch('/xtgl/zdpz_cxZdpzList.html?gnmkdm=N2151&doType=query', {
     method: 'POST',
     headers: {
       'X-Requested-With': 'XMLHttpRequest',
       'Content-Type': 'application/x-www-form-urlencoded;charset=UTF-8',
     },
     body: 'zdmc=jc&_search=false&nd=' + Date.now()
   });
   return await r.text();
   ```
3. 若 404 / 500 → 候选 2：`/xtgl/comm_cxZdpzList.html`、`/cdjcgl/cdjcsj_cxCdjcsjLb.html`
4. 若全部失败 → 用 SJTU 教务处公开页面（[https://jwc.sjtu.edu.cn/](https://jwc.sjtu.edu.cn/)）人工录入节次表

**输出**：1-13 节次对应的 `(start_hh:mm, end_hh:mm)` 元组列表。**记录到 `tasks/isjtu_investigation.md` §2.7 末尾**，附信源 URL。

- [ ] **Step 2：真机抓 N2154 zs=1 响应**

在已登录 SJTU 的 chrome 里执行只读 fetch：

```javascript
const r = await fetch('/kbcx/xskbcxMobile_cxXsKb.html?gnmkdm=N2154', {
  method: 'POST',
  headers: {
    'X-Requested-With': 'XMLHttpRequest',
    'Content-Type': 'application/x-www-form-urlencoded;charset=UTF-8',
  },
  body: 'xnm=2025&xqm=12&zs=1&kblx=1&doType=app&xh='
});
return await r.text();
```

注意：`/kbcx/xskbcxMobile_cxXsKb.html` 是调研值，若 404 → 改 `xskbcx_cxXsgrkb.html`（N2151 同 path）+ 加 `zs` 字段重测。

- [ ] **Step 3：脱敏 + 落 fixture**

把 step 2 响应 JSON 写到 `tests/fixtures/jwc/n2154_week_zs1.json`，把 `xh` / `kch_id` / `xm` / `cdmc` / `jxbmc` 全部替换为 dummy 值（`"DUMMY_XH"` / `"FL1405-01"` / `"张老师"` / `"东中院 1-101"`）。**确认无学号 / 真实姓名残留**。

- [ ] **Step 4：抓 zs=14 同样脱敏落 `n2154_week_zs14.json`**

- [ ] **Step 5：把节次时刻表落到 T4 的 `period_clock.rs` 注释里**（T4 实装时直接抄）

```
// 待 T4 任务填入，格式如：
// const DEFAULT_TABLE: [(NaiveTime, NaiveTime); 13] = [
//     (NaiveTime::from_hms_opt(8, 0, 0).unwrap(),  NaiveTime::from_hms_opt(8, 45, 0).unwrap()),   // 第 1 节
//     ...
// ];
```

- [ ] **Step 6：Commit**

```bash
git add tests/fixtures/jwc/ tasks/isjtu_investigation.md
git commit -m "chore(jwc): T0 调研 ZF 节次字典 + 真机抓 N2154 fixture (zs=1/zs=14)"
```

---

## Task T1：Cargo.toml 加 comfy-table 依赖

**Files:**
- Modify: `Cargo.toml:48`（在 `rust_decimal` 行之后）

- [ ] **Step 1：写依赖行**

在 `Cargo.toml` 的 `[dependencies]` section 末尾（`rust_decimal` 行之后）加一行：

```toml
# --- T1 jwc 课表 grid 渲染 ---
# 锁 7.1.x：7.2 MSRV 1.85 超项目 rust-version 1.75
comfy-table = "~7.1"
```

- [ ] **Step 2：cargo check 通过**

```bash
cargo check 2>&1 | tail -10
```

Expected: `Finished dev [unoptimized + debuginfo] target(s) in X.XXs`，无 error。

- [ ] **Step 3：cargo fmt + clippy**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: 无 warning。

- [ ] **Step 4：Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "feat(jwc): add comfy-table ~7.1 for grid schedule output"
```

---

## Task T2：config.rs 加 cache_dir helper

**Files:**
- Modify: `src/config.rs`（当前 60 行 → ~95 行）
- Test: 同文件 `#[cfg(test)] mod tests`

- [ ] **Step 1：写 failing test**

在 `src/config.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_dir_returns_a_path() {
        let path = cache_dir().expect("cache_dir 失败");
        assert!(!path.as_os_str().is_empty(), "cache_dir 不能返回空路径");
    }

    #[test]
    fn jwc_week_cache_path_ends_with_correct_filename() {
        let path = jwc_week_cache_path().expect("jwc_week_cache_path 失败");
        assert_eq!(
            path.file_name().and_then(|s| s.to_str()),
            Some("jwc_week_cache.json"),
            "文件名必须是 jwc_week_cache.json"
        );
    }

    #[test]
    fn cache_dir_differs_from_config_dir() {
        let c = config_dir().unwrap();
        let cc = cache_dir().unwrap();
        assert_ne!(
            c, cc,
            "cache_dir 必须和 config_dir 是不同目录（XDG / OS 标准要求）"
        );
    }
}
```

- [ ] **Step 2：跑测试确认 fail**

```bash
cargo test --lib config::tests 2>&1 | tail -15
```

Expected: 3 个 test 都 fail（`cache_dir` / `jwc_week_cache_path` 还不存在）。

- [ ] **Step 3：实装 cache_dir 系列函数**

在 `src/config.rs` 的 `sub_sessions_dir()` 函数之后（约第 30 行）插入：

```rust
/// 解析平台 cache 目录（与 config_dir 分离，遵循 XDG / OS 标准）。
///
/// 平台路径（由 `directories` crate 决定）：
/// - Linux:   `~/.cache/sjtu-cli/`
/// - macOS:   `~/Library/Caches/edu.sjtu.sjtu-cli/`
/// - Windows: `%LOCALAPPDATA%\sjtu\sjtu-cli\cache\`
pub fn cache_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("edu", "sjtu", "sjtu-cli")
        .context("无法解析平台 cache 目录（HOME / LOCALAPPDATA 未设置？）")?;
    Ok(dirs.cache_dir().to_path_buf())
}

/// jwc 周次反推 cache 文件路径：`<cache_dir>/jwc_week_cache.json`。
pub fn jwc_week_cache_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("jwc_week_cache.json"))
}

/// 幂等创建 cache 目录（700 权限 on Unix）。
pub fn ensure_cache_dir() -> Result<()> {
    let dir = cache_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("无法创建 cache 目录 {}", dir.display()))?;
    #[cfg(unix)]
    set_unix_dir_perm(&dir)?;
    Ok(())
}
```

- [ ] **Step 4：跑测试确认 pass**

```bash
cargo test --lib config::tests 2>&1 | tail -15
```

Expected: `test result: ok. 3 passed`。

- [ ] **Step 5：cargo fmt + clippy**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: 无 warning。

- [ ] **Step 6：Commit**

```bash
git add src/config.rs
git commit -m "feat(config): add cache_dir / jwc_week_cache_path / ensure_cache_dir"
```

---

## Task T3：models/schedule.rs 扩 KbItem + RqAzc + Schedule.rqazc_list

**⚠️ T0 真机校正（不要照 4-26 旧 §2.7 jq 推断的类型实装）：**
- `rqazcList[*].xqj` 真机是 **number** (`1..7`)，不是 string
- `kbList[*].oldzc` / `oldjc` 真机是 **string** (`"65535"` / `"12"`)，不是 number；Rust 用 `deserialize_with` parse 成 `u32`
- 信源：`tests/fixtures/jwc/n2154_week_zs1.json` 真机 fixture（T0 抓）

**Files:**
- Modify: `src/apps/jwc/models/schedule.rs`（106 行 → ~180 行）

- [ ] **Step 1：写 failing test**

在 `src/apps/jwc/models/schedule.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_deserializes_n2154_rqazc_list() {
        // 真机 N2154：xqj 是 number (1..7)，rq 是 ISO 字符串
        let json = r#"{
            "xqjmcMap": {"1":"星期一","2":"星期二"},
            "kbList": [],
            "rqazcList": [
                {"rq": "2025-09-08", "xqj": 1},
                {"rq": "2025-09-09", "xqj": 2}
            ]
        }"#;
        let s: Schedule = serde_json::from_str(json).expect("Schedule 解析失败");
        assert_eq!(s.rqazc_list.len(), 2);
        assert_eq!(s.rqazc_list[0].rq.as_deref(), Some("2025-09-08"));
        assert_eq!(s.rqazc_list[0].xqj, Some(1));
    }

    #[test]
    fn kb_item_deserializes_old_zc_old_jc_from_string() {
        // 真机 N2154：oldzc / oldjc 是 string，需 parse 成 u32
        let json = r#"{"kcmc":"高数","oldzc":"524286","oldjc":"12"}"#;
        let k: KbItem = serde_json::from_str(json).expect("KbItem 解析失败");
        assert_eq!(k.old_zc, Some(524286));
        assert_eq!(k.old_jc, Some(12));
    }

    #[test]
    fn kb_item_old_zc_absent_defaults_to_none() {
        // N2151 路径无 oldzc / oldjc
        let json = r#"{"kcmc":"高数"}"#;
        let k: KbItem = serde_json::from_str(json).expect("KbItem 解析失败");
        assert!(k.old_zc.is_none());
        assert!(k.old_jc.is_none());
    }

    #[test]
    fn n2151_response_without_rqazc_list_defaults_to_empty_vec() {
        let json = r#"{"xqjmcMap":{},"kbList":[]}"#;
        let s: Schedule = serde_json::from_str(json).expect("Schedule 解析失败");
        assert!(s.rqazc_list.is_empty(), "N2151 路径 rqazc_list 必须为空 Vec");
    }
}
```

- [ ] **Step 2：跑测试确认 fail**

```bash
cargo test --lib apps::jwc::models::schedule::tests 2>&1 | tail -15
```

Expected: 3 个 test fail（`old_zc` / `old_jc` / `rqazc_list` 字段不存在）。

- [ ] **Step 3：实装字段扩展**

修改 `src/apps/jwc/models/schedule.rs`：

3.1 在文件末尾（`KbItem` struct 之后）插入新 struct：

```rust
/// §2.7 N2154 `rqazcList[*]` 单条：周次对应的日期 + 周几。
///
/// N2151 路径下永远空（`serde(default)` 兼容）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RqAzc {
    /// 日期 ISO 字符串（"2025-09-08"）。
    #[serde(default)]
    pub rq: Option<String>,
    /// 周几（**真机是 number 1..7**，非 string）。
    #[serde(default)]
    pub xqj: Option<u8>,
}
```

3.2 在文件顶部 `use` 区域加 helper（用于 KbItem.old_zc / old_jc 从 string parse）：

```rust
use serde::{Deserialize, Deserializer, Serialize};

/// 反序列化 string ("65535") → Option<u32>。
/// 兼容 N2154 真机返回（`oldzc` / `oldjc` 是 string，不是 number）。
fn deserialize_opt_u32_from_str<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) if s.is_empty() => Ok(None),
        Some(s) => s.parse::<u32>().map(Some).map_err(serde::de::Error::custom),
    }
}
```

（若 `use serde::Deserialize` 已存在，把上面的改成补 `Deserializer` 即可。）

3.3 在 `Schedule` struct 里加 `rqazc_list` 字段（紧跟 `kb_list` 后）：

```rust
    /// 课表条目（已按周几+节次铺平）。
    #[serde(default)]
    pub kb_list: Vec<KbItem>,

    /// §2.7 N2154 周课表的"周次→日期"映射；N2151 路径永远空 Vec。
    #[serde(default, rename = "rqazcList")]
    pub rqazc_list: Vec<RqAzc>,
```

3.4 在 `KbItem` struct 末尾（`skfsmc` 字段之后）加：

```rust
    /// §2.7 N2154 周次 bitmask（位 0=第 1 周）。N2151 路径下永远 None。
    /// **真机 JSON 是 string ("65535")**，用 `deserialize_with` 转 u32。
    #[serde(default, rename = "oldzc", deserialize_with = "deserialize_opt_u32_from_str")]
    pub old_zc: Option<u32>,
    /// §2.7 N2154 节次 bitmask（位 0=第 1 节）。N2151 路径下永远 None。
    /// **真机 JSON 是 string ("12")**，用 `deserialize_with` 转 u32。
    #[serde(default, rename = "oldjc", deserialize_with = "deserialize_opt_u32_from_str")]
    pub old_jc: Option<u32>,
```

- [ ] **Step 4：跑测试确认 pass**

```bash
cargo test --lib apps::jwc::models::schedule::tests 2>&1 | tail -15
```

Expected: `test result: ok. 3 passed`。

- [ ] **Step 5：cargo fmt + clippy**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: 无 warning。

- [ ] **Step 6：行数检查**

```bash
wc -l src/apps/jwc/models/schedule.rs
```

Expected: ≤ 160 行（< 200 硬限）。

- [ ] **Step 7：Commit**

```bash
git add src/apps/jwc/models/schedule.rs
git commit -m "feat(jwc): extend KbItem with old_zc/old_jc + add RqAzc + Schedule.rqazc_list"
```

---

## Task T4：period_clock.rs 新建

**Files:**
- Create: `src/apps/jwc/period_clock.rs`
- Modify: `src/apps/jwc/mod.rs`（加 `pub mod period_clock;`）

- [ ] **Step 1：写 failing test**

创建 `src/apps/jwc/period_clock.rs`，只写 test 模块：

```rust
//! SJTU 节次 → 起止时刻映射（fallback 硬编码 1-13 节 + 公开 lookup / bitmask helpers）。
//!
//! 节次时刻来源：tasks/isjtu_investigation.md §2.7 末尾（T0 调研结果）。
//! 若 T0 调研未完成 / 字典端点 404，则用 SJTU 教务处公开页面值（信源 URL in 注释）。

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_jc_1_returns_8_00_to_8_45() {
        let (s, e) = lookup(1).expect("第 1 节必须存在");
        assert_eq!(s.format("%H:%M").to_string(), "08:00");
        assert_eq!(e.format("%H:%M").to_string(), "08:45");
    }

    #[test]
    fn lookup_jc_0_returns_none() {
        assert!(lookup(0).is_none(), "第 0 节非法");
    }

    #[test]
    fn lookup_jc_14_returns_none() {
        assert!(lookup(14).is_none(), "第 14 节超表");
    }

    #[test]
    fn is_in_week_old_zc_524286_returns_true_for_week_2() {
        // 524286 = 0b1111111111111111110，位 1 = 第 2 周
        assert!(is_in_week(524286, 2));
        assert!(is_in_week(524286, 18));
        assert!(!is_in_week(524286, 1), "位 0 = 0，第 1 周不在");
    }

    #[test]
    fn is_in_week_week_0_returns_false() {
        assert!(!is_in_week(0xFFFF, 0), "第 0 周非法");
    }

    #[test]
    fn is_in_week_week_33_returns_false() {
        assert!(!is_in_week(0xFFFFFFFF, 33), "超 32 位非法");
    }

    #[test]
    fn jc_positions_old_jc_12_returns_3_and_4() {
        // 12 = 0b1100，位 2/3 = 第 3/4 节
        assert_eq!(jc_positions(12), vec![3, 4]);
    }

    #[test]
    fn jc_positions_old_jc_0_returns_empty() {
        assert!(jc_positions(0).is_empty());
    }
}
```

- [ ] **Step 2：跑测试确认 fail**

```bash
cargo test --lib apps::jwc::period_clock 2>&1 | tail -10
```

Expected: 编译失败（`super::lookup` / `is_in_week` / `jc_positions` 不存在）。

- [ ] **Step 3：实装 lookup + bitmask helpers**

把 `src/apps/jwc/period_clock.rs` 内容替换为：

```rust
//! SJTU 节次 → 起止时刻映射（fallback 硬编码 1-13 节 + 公开 lookup / bitmask helpers）。
//!
//! 节次时刻来源：tasks/isjtu_investigation.md §2.7 末尾（T0 调研结果）。
//! 若 T0 调研未完成 / 字典端点 404，则用 SJTU 教务处公开页面值。
//! 信源：https://jwc.sjtu.edu.cn/ （首页底部"作息时间表"链接 / 真机 fallback）

use chrono::NaiveTime;

/// SJTU 闵行 / 徐汇主区作息（13 节制）。
/// 信源：T0 调研落定（tasks/isjtu_investigation.md §2.7 末尾）
/// - 上海交通大学教务处《学生上课时间表》https://jwc.sjtu.edu.cn/info/1041/1110.htm
/// - 上海交通大学设计学院《上课节次及时间对照表》两信源 1-12 节完全一致
/// - 第 13 节单节 19:50-20:35 按 12 节后 10 分钟休息推算（官方仅给 11-13 连上 18:00-20:20）
const DEFAULT_TABLE: [(u32, u32, u32, u32); 13] = [
    // (start_h, start_m, end_h, end_m)
    (8, 0, 8, 45),    // 第 1 节
    (8, 55, 9, 40),   // 第 2 节
    (10, 0, 10, 45),  // 第 3 节
    (10, 55, 11, 40), // 第 4 节
    (12, 0, 12, 45),  // 第 5 节
    (12, 55, 13, 40), // 第 6 节
    (14, 0, 14, 45),  // 第 7 节
    (14, 55, 15, 40), // 第 8 节
    (16, 0, 16, 45),  // 第 9 节
    (16, 55, 17, 40), // 第 10 节
    (18, 0, 18, 45),  // 第 11 节
    (18, 55, 19, 40), // 第 12 节
    (19, 50, 20, 35), // 第 13 节（推算）
];

/// 第 `jc` 节（1-13）的起止时刻；越界返回 None。
pub fn lookup(jc: u8) -> Option<(NaiveTime, NaiveTime)> {
    if jc == 0 || jc as usize > DEFAULT_TABLE.len() {
        return None;
    }
    let (sh, sm, eh, em) = DEFAULT_TABLE[jc as usize - 1];
    Some((
        NaiveTime::from_hms_opt(sh, sm, 0)?,
        NaiveTime::from_hms_opt(eh, em, 0)?,
    ))
}

/// `old_zc` bitmask（位 0=第 1 周）是否包含第 `week` 周。
pub fn is_in_week(old_zc: u32, week: u8) -> bool {
    if week == 0 || week > 32 {
        return false;
    }
    (old_zc >> (week - 1)) & 1 == 1
}

/// `old_jc` bitmask（位 0=第 1 节）展开成节次列表。
pub fn jc_positions(old_jc: u32) -> Vec<u8> {
    (0u32..32)
        .filter(|i| (old_jc >> i) & 1 == 1)
        .map(|i| (i + 1) as u8)
        .collect()
}
```

- [ ] **Step 4：在 mod.rs 里 declare 模块**

修改 `src/apps/jwc/mod.rs`，在 `mod throttle;` 行之后插入：

```rust
pub mod period_clock;
```

并在 `pub use api::...` 后加 re-export（可选）：

```rust
pub use models::{Exam, Gpa, Grade, JwcPage, KbItem, RqAzc, Schedule};
```

（注意：要把 `RqAzc` 加到 re-export，T9 的 `schedule_handlers.rs` 会用到。）

- [ ] **Step 5：跑测试确认 pass**

```bash
cargo test --lib apps::jwc::period_clock 2>&1 | tail -15
```

Expected: `test result: ok. 8 passed`。

- [ ] **Step 6：cargo fmt + clippy**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: 无 warning。

- [ ] **Step 7：行数检查**

```bash
wc -l src/apps/jwc/period_clock.rs
```

Expected: ≤ 120 行。

- [ ] **Step 8：Commit**

```bash
git add src/apps/jwc/period_clock.rs src/apps/jwc/mod.rs
git commit -m "feat(jwc): add period_clock module (1-13 节 fallback table + bitmask helpers)"
```

---

## Task T5：api/schedule.rs 加 schedule_by_week

**Files:**
- Modify: `src/apps/jwc/api/schedule.rs`（42 行 → ~85 行）

- [ ] **Step 1：写 failing test**

新建 `src/apps/jwc/api/schedule.rs` 末尾（在现有 `schedule()` 函数之后）插入 test：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 N2154 form 是否拼对（字段顺序 + zs 数值）。
    /// 由于真实调用需要 SJTU session，这里只测 form 拼装是 pure 函数式。
    #[test]
    fn n2154_form_contains_required_fields() {
        let form = build_n2154_form(Some("2025"), Some("12"), 14);
        assert!(form.iter().any(|(k, v)| *k == "xnm" && v == "2025"));
        assert!(form.iter().any(|(k, v)| *k == "xqm" && v == "12"));
        assert!(form.iter().any(|(k, v)| *k == "zs" && v == "14"));
        assert!(form.iter().any(|(k, v)| *k == "kblx" && v == "1"));
        assert!(form.iter().any(|(k, v)| *k == "doType" && v == "app"));
        assert!(form.iter().any(|(k, v)| *k == "xh" && v.is_empty()));
    }

    #[test]
    fn n2154_form_empty_xnm_xqm_default_to_empty_string() {
        let form = build_n2154_form(None, None, 1);
        assert!(form.iter().any(|(k, v)| *k == "xnm" && v.is_empty()));
        assert!(form.iter().any(|(k, v)| *k == "xqm" && v.is_empty()));
    }
}
```

- [ ] **Step 2：跑测试确认 fail**

```bash
cargo test --lib apps::jwc::api::schedule::tests 2>&1 | tail -10
```

Expected: 编译失败（`build_n2154_form` 不存在）。

- [ ] **Step 3：实装 schedule_by_week + build_n2154_form**

修改 `src/apps/jwc/api/schedule.rs`，在 `impl Client { ... }` block 内、`schedule()` 函数之后追加：

```rust
    /// §2.7 N2154 周次课表查询。返回的 envelope 含 `rqazcList`（每天 ISO 日期）+
    /// `kbList[*].oldzc/oldjc` 周次/节次 bitmask。
    ///
    /// `zs` 必填（1..=18 教学周）；`xnm`/`xqm` 留空 = 当前学年/学期。
    pub async fn schedule_by_week(
        &self,
        xnm: Option<&str>,
        xqm: Option<&str>,
        zs: u8,
    ) -> Result<Schedule> {
        self.ensure_sp_bound(
            "/kbcx/xskbcxMobile_cxXskbcxMobileIndex.html",
            "N2154",
            "N2154 周课表",
        )
        .await?;

        let form = build_n2154_form(xnm, xqm, zs);
        post_form_json(
            &self.http,
            &self.throttle,
            "/kbcx/xskbcxMobile_cxXsKb.html",
            "N2154",
            None, // §2.7：端点不带 doType
            "/kbcx/xskbcxMobile_cxXskbcxMobileIndex.html",
            &form,
            "N2154 周课表",
        )
        .await
    }
}

/// 拼 N2154 form（pure 函数，便于单测）。
fn build_n2154_form(xnm: Option<&str>, xqm: Option<&str>, zs: u8) -> Vec<(&'static str, String)> {
    vec![
        ("xnm", xnm.unwrap_or("").to_string()),
        ("xqm", xqm.unwrap_or("").to_string()),
        ("zs", zs.to_string()),
        ("kblx", "1".to_string()),
        ("doType", "app".to_string()),
        ("xh", String::new()),
    ]
}
```

**注意**：上面 `impl Client { ... }` block 的 `}` 必须在 `build_n2154_form` 之前，确保 helper 是模块级函数。看现有文件，`schedule()` 已经在 `impl Client { ... }` 内，新加的 `schedule_by_week` 在同 impl 内，然后 impl 闭合 → 模块级 `fn build_n2154_form`。

- [ ] **Step 4：跑测试确认 pass**

```bash
cargo test --lib apps::jwc::api::schedule::tests 2>&1 | tail -15
```

Expected: `test result: ok. 2 passed`。

- [ ] **Step 5：cargo fmt + clippy**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: 无 warning。

- [ ] **Step 6：行数检查**

```bash
wc -l src/apps/jwc/api/schedule.rs
```

Expected: ≤ 100 行（< 200 硬限，含 T6 后约 ~135 行仍 OK）。

- [ ] **Step 7：Commit**

```bash
git add src/apps/jwc/api/schedule.rs
git commit -m "feat(jwc): add schedule_by_week (N2154) + build_n2154_form helper"
```

---

## Task T6：api/schedule.rs 加 infer_current_week + cache I/O

**Files:**
- Modify: `src/apps/jwc/api/schedule.rs`（T5 后 ~85 行 → ~165 行）

- [ ] **Step 1：写 failing test**

在 `src/apps/jwc/api/schedule.rs` 的 `mod tests { ... }` 内追加：

```rust
    use chrono::NaiveDate;

    #[test]
    fn compute_current_week_2025_09_08_to_2026_05_12_returns_36() {
        let week1_monday = NaiveDate::from_ymd_opt(2025, 9, 8).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let cw = compute_current_week(week1_monday, today);
        // (2026-05-12) - (2025-09-08) = 246 天；246 / 7 = 35.14 → cw = 36
        assert_eq!(cw, 36);
    }

    #[test]
    fn compute_current_week_same_day_returns_1() {
        let d = NaiveDate::from_ymd_opt(2025, 9, 8).unwrap();
        assert_eq!(compute_current_week(d, d), 1);
    }

    #[test]
    fn compute_current_week_before_semester_returns_0() {
        let week1 = NaiveDate::from_ymd_opt(2025, 9, 8).unwrap();
        let pre = NaiveDate::from_ymd_opt(2025, 9, 1).unwrap(); // 1 周前
        assert_eq!(compute_current_week(week1, pre), 0);
    }

    #[test]
    fn cache_key_uses_current_when_xnm_xqm_none() {
        assert_eq!(cache_key(None, None), "__current__");
        assert_eq!(cache_key(Some("2025"), None), "__current__");
        assert_eq!(cache_key(None, Some("12")), "__current__");
    }

    #[test]
    fn cache_key_uses_xnm_xqm_when_both_given() {
        assert_eq!(cache_key(Some("2025"), Some("12")), "2025-12");
    }

    #[test]
    fn cache_ttl_1h_for_current_24h_for_explicit() {
        assert_eq!(cache_ttl_seconds(None, None), 3600);
        assert_eq!(cache_ttl_seconds(Some("2025"), Some("12")), 86400);
    }
```

- [ ] **Step 2：跑测试确认 fail**

```bash
cargo test --lib apps::jwc::api::schedule::tests 2>&1 | tail -15
```

Expected: 6 个新 test 编译失败（4 个新函数不存在）。

- [ ] **Step 3：实装 4 个 pure helpers**

在 `src/apps/jwc/api/schedule.rs` 的 `build_n2154_form` 之后追加：

```rust
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 根据第 1 周周一日期与今日日期，计算当前是第几教学周。
/// 负差返回 0（表示学期未开始）。
pub fn compute_current_week(week1_monday: NaiveDate, today: NaiveDate) -> u8 {
    let delta_days = (today - week1_monday).num_days();
    if delta_days < 0 {
        return 0;
    }
    ((delta_days / 7) + 1) as u8
}

/// cache key：显式 xnm+xqm → "{xnm}-{xqm}"；否则 "__current__"。
pub fn cache_key(xnm: Option<&str>, xqm: Option<&str>) -> String {
    match (xnm, xqm) {
        (Some(x), Some(q)) => format!("{x}-{q}"),
        _ => "__current__".to_string(),
    }
}

/// cache TTL：显式 24h，"__current__" 1h（避免学期切换误判）。
pub fn cache_ttl_seconds(xnm: Option<&str>, xqm: Option<&str>) -> i64 {
    match (xnm, xqm) {
        (Some(_), Some(_)) => 86_400,
        _ => 3_600,
    }
}

/// jwc_week_cache.json 内容。key = cache_key, value = (week, fetched_at_ISO)。
#[derive(Debug, Default, Serialize, Deserialize)]
struct WeekCache {
    #[serde(default)]
    entries: BTreeMap<String, CacheEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    week: u8,
    /// ISO datetime with timezone（chrono::Utc::now().to_rfc3339()）。
    fetched_at: String,
}
```

3.2 同文件继续追加 `infer_current_week` 公共函数 + cache I/O helpers：

```rust
use chrono::{DateTime, Utc};

impl Client {
    /// 反推今天属于第几教学周。优先读 cache（TTL 内）；miss → 调 N2154 zs=1 算 delta。
    ///
    /// 返回 0 表示学期未开始（hint："学期未开始"）；超 18 表示假期（hint："学期已结束/假期"）。
    pub async fn infer_current_week(
        &self,
        xnm: Option<&str>,
        xqm: Option<&str>,
    ) -> Result<u8> {
        let key = cache_key(xnm, xqm);
        let ttl = cache_ttl_seconds(xnm, xqm);

        if let Some(cw) = read_cache_if_fresh(&key, ttl) {
            return Ok(cw);
        }

        let s = self.schedule_by_week(xnm, xqm, 1).await?;
        let week1_iso = s
            .rqazc_list
            .iter()
            .find_map(|r| r.rq.as_deref().filter(|_| r.xqj.as_deref() == Some("1")))
            .or_else(|| s.rqazc_list.first().and_then(|r| r.rq.as_deref()))
            .ok_or_else(|| {
                anyhow::anyhow!("N2154 zs=1 响应缺少 rqazcList[*].rq，无法反推当前周")
            })?;

        let week1_monday = NaiveDate::parse_from_str(week1_iso, "%Y-%m-%d")
            .map_err(|e| anyhow::anyhow!("rqazcList[0].rq '{week1_iso}' 非 ISO 日期: {e}"))?;
        let today = chrono::Local::now().date_naive();
        let cw = compute_current_week(week1_monday, today);

        let _ = write_cache(&key, cw);
        Ok(cw)
    }
}

/// 读 cache，若 entry 存在且未超 TTL → 返回 week；否则 None。
fn read_cache_if_fresh(key: &str, ttl_seconds: i64) -> Option<u8> {
    let path = crate::config::jwc_week_cache_path().ok()?;
    let bytes = std::fs::read(&path).ok()?;
    let cache: WeekCache = serde_json::from_slice(&bytes).ok()?;
    let entry = cache.entries.get(key)?;
    let fetched: DateTime<Utc> = entry.fetched_at.parse().ok()?;
    let age = (Utc::now() - fetched).num_seconds();
    if age >= 0 && age < ttl_seconds {
        Some(entry.week)
    } else {
        None
    }
}

/// 写 cache（best-effort，错误吞掉只记 tracing）。
fn write_cache(key: &str, week: u8) -> Result<()> {
    crate::config::ensure_cache_dir()?;
    let path = crate::config::jwc_week_cache_path()?;
    let mut cache: WeekCache = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    cache.entries.insert(
        key.to_string(),
        CacheEntry {
            week,
            fetched_at: Utc::now().to_rfc3339(),
        },
    );
    let bytes = serde_json::to_vec_pretty(&cache)?;
    std::fs::write(&path, bytes)?;
    Ok(())
}
```

- [ ] **Step 4：跑测试确认 pass**

```bash
cargo test --lib apps::jwc::api::schedule::tests 2>&1 | tail -20
```

Expected: 8 个 test 全 pass（T5 的 2 个 + T6 的 6 个 pure helper test）。

- [ ] **Step 5：cargo fmt + clippy**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | tail -10
```

Expected: 无 warning。注：若 clippy 报 `match (xnm, xqm) { ... }` 风格 → 可改 `if let (Some(x), Some(q)) = (xnm, xqm) { ... } else { ... }`。

- [ ] **Step 6：行数检查**

```bash
wc -l src/apps/jwc/api/schedule.rs
```

Expected: ≤ 200 行（< 硬限）。若接近 200 → 提示后续 T7 拆 cache I/O 到独立文件。

- [ ] **Step 7：Commit**

```bash
git add src/apps/jwc/api/schedule.rs
git commit -m "feat(jwc): add infer_current_week + cache I/O (TTL 24h explicit / 1h __current__)"
```

---

## Task T7：拆 src/cli/jwc.rs → src/cli/jwc/{mod,schedule_cli}.rs

**Files:**
- Delete: `src/cli/jwc.rs`
- Create: `src/cli/jwc/mod.rs`
- Create: `src/cli/jwc/schedule_cli.rs`

> **重要**：clap 的 nested module pattern 已联网验证为常见做法（参考 [clap docs](https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html)）。`src/cli/jwc/mod.rs` 作为模块入口，`schedule_cli.rs` 提供 `Today/Week/Next` 三个 subcommand 的 args struct。**`JwcSub` enum 必须留在 `mod.rs`**（因为它是 pub use 给 `src/cli/mod.rs` 用的），但 `Today/Week/Next` variant 的字段通过 `#[derive(Args)] struct ...` 拆到 `schedule_cli.rs`。

- [ ] **Step 1：写 failing test（clap 自检）**

创建 `tests/cli_jwc_schedule_clap.rs`（项目根 tests/）：

```rust
//! `sjtu jwc today / week / next` clap 解析自检。

use clap::Parser;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    sub: sjtu_cli::cli::jwc::JwcSub,
}

#[test]
fn today_parses_with_no_args() {
    let _ = Cli::parse_from(["sjtu", "today"]);
}

#[test]
fn today_parses_with_grid_flag() {
    let _ = Cli::parse_from(["sjtu", "today", "--grid"]);
}

#[test]
fn week_parses_with_zs_and_xnm() {
    let _ = Cli::parse_from(["sjtu", "week", "--zs", "14", "--xnm", "2025"]);
}

#[test]
fn week_rejects_zs_0() {
    let r = Cli::try_parse_from(["sjtu", "week", "--zs", "0"]);
    assert!(r.is_err(), "--zs 0 应被 clap range 拒绝");
}

#[test]
fn week_rejects_zs_19() {
    let r = Cli::try_parse_from(["sjtu", "week", "--zs", "19"]);
    assert!(r.is_err(), "--zs 19 应被 clap range 拒绝");
}

#[test]
fn next_parses_with_within_and_limit() {
    let _ = Cli::parse_from(["sjtu", "next", "--within", "7", "--limit", "20"]);
}

#[test]
fn next_rejects_within_32() {
    let r = Cli::try_parse_from(["sjtu", "next", "--within", "32"]);
    assert!(r.is_err(), "--within 32 应被 clap range 拒绝");
}
```

**前置**：需要 `src/cli/mod.rs` 的 `mod jwc;` 改成 `pub mod jwc;` + `src/cli/jwc/mod.rs` 把 `JwcSub` 改成 `pub`。`src/lib.rs` 已 `pub mod cli;`（如未 pub 需先改）。

- [ ] **Step 2：跑测试确认 fail**

```bash
cargo test --test cli_jwc_schedule_clap 2>&1 | tail -10
```

Expected: 编译失败（`Today/Week/Next` variant 不存在）。

- [ ] **Step 3：删旧 jwc.rs**

```bash
rm src/cli/jwc.rs
```

- [ ] **Step 4：写新 src/cli/jwc/mod.rs**

```rust
//! `sjtu jwc <sub>` 教务系统命令的 clap 枚举 + 派发（拆分入口）。
//!
//! 命令清单（按 §2 顺序 + T1 衍生）：
//! - `grades`   — §2.1 N305005 学生成绩查询
//! - `schedule` — §2.2 N2151 学年学期课表
//! - `gpa`      — §2.3 N309131 GPA / 学积分（两阶段）
//! - `exams`    — §2.4 N358105 考试信息查询
//! - `today`    — T1 §2.7 N2154 今日剩余课
//! - `week`     — T1 §2.7 N2154 整周课表
//! - `next`     — T1 §2.7 N2154 接下来 N 天的课
//!
//! `Today/Week/Next` 三个 variant 的字段定义在 `schedule_cli.rs`。

use anyhow::Result;
use clap::{Subcommand, ValueEnum};

use crate::apps::jwc::{GpaRank, GpaScope};
use crate::commands::jwc as jwc_cmds;
use crate::output::OutputFormat;

mod schedule_cli;
pub use schedule_cli::{NextArgs, TodayArgs, WeekArgs};

/// `sjtu jwc gpa --scope` 的 ValueEnum。
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum GpaScopeArg {
    /// 核心课程（默认）。
    Hxkc,
    /// 全部课程。
    Qbkc,
}

impl From<GpaScopeArg> for GpaScope {
    fn from(s: GpaScopeArg) -> Self {
        match s {
            GpaScopeArg::Hxkc => GpaScope::HxKc,
            GpaScopeArg::Qbkc => GpaScope::QbKc,
        }
    }
}

/// `sjtu jwc gpa --rank` 的 ValueEnum。
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum GpaRankArg {
    /// 年级专业（默认）。
    Njzy,
    /// 年级。
    Nj,
    /// 班级。
    Bj,
}

impl From<GpaRankArg> for GpaRank {
    fn from(r: GpaRankArg) -> Self {
        match r {
            GpaRankArg::Njzy => GpaRank::NjZy,
            GpaRankArg::Nj => GpaRank::Nj,
            GpaRankArg::Bj => GpaRank::Bj,
        }
    }
}

/// `sjtu jwc <sub>` 子命令集合。
#[derive(Debug, Subcommand)]
pub enum JwcSub {
    /// 查询成绩（N305005）。`--xnm`/`--xqm` 留空 = 查全部。
    Grades {
        #[arg(long)]
        xnm: Option<String>,
        #[arg(long)]
        xqm: Option<String>,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },

    /// 查询课表（N2151，学年学期视图）。`--xnm`/`--xqm` 留空 = 当前学年/学期。
    Schedule {
        #[arg(long)]
        xnm: Option<String>,
        #[arg(long)]
        xqm: Option<String>,
    },

    /// 查询 GPA / 学积分（N309131，两阶段触发统计 + 拉结果）。
    Gpa {
        #[arg(long, value_enum, default_value_t = GpaScopeArg::Hxkc)]
        scope: GpaScopeArg,
        #[arg(long, value_enum, default_value_t = GpaRankArg::Njzy)]
        rank: GpaRankArg,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
    },

    /// 查询考试信息（N358105）。
    Exams {
        #[arg(long)]
        xnm: Option<String>,
        #[arg(long)]
        xqm: Option<String>,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },

    /// 今日剩余的课（自动反推当前周）。
    Today(TodayArgs),

    /// 整周课表（默认当前周，可 `--zs N` 指定）。
    Week(WeekArgs),

    /// 接下来若干天的课（默认 within=1 = 今天剩余）。
    Next(NextArgs),
}

/// 派发 `sjtu jwc <sub>` 到 `commands::jwc` 的 handler。
pub async fn dispatch(sub: JwcSub, fmt: Option<OutputFormat>) -> Result<()> {
    match sub {
        JwcSub::Grades {
            xnm,
            xqm,
            page,
            limit,
        } => jwc_cmds::cmd_grades(xnm, xqm, page, limit, fmt).await,
        JwcSub::Schedule { xnm, xqm } => jwc_cmds::cmd_schedule(xnm, xqm, fmt).await,
        JwcSub::Gpa {
            scope,
            rank,
            from,
            to,
        } => jwc_cmds::cmd_gpa(scope.into(), rank.into(), from, to, fmt).await,
        JwcSub::Exams {
            xnm,
            xqm,
            page,
            limit,
        } => jwc_cmds::cmd_exams(xnm, xqm, page, limit, fmt).await,
        JwcSub::Today(a) => jwc_cmds::cmd_today(a.xnm, a.xqm, a.grid, fmt).await,
        JwcSub::Week(a) => jwc_cmds::cmd_week(a.xnm, a.xqm, a.zs, a.grid, fmt).await,
        JwcSub::Next(a) => jwc_cmds::cmd_next(a.xnm, a.xqm, a.within, a.limit, fmt).await,
    }
}
```

- [ ] **Step 5：写 src/cli/jwc/schedule_cli.rs**

```rust
//! `sjtu jwc today / week / next` 三个衍生命令的 args struct（拆出来守 200 行硬限）。

use clap::Args;

/// `sjtu jwc today [--xnm 2025] [--xqm 12] [--grid]`：今日剩余的课。
#[derive(Debug, Args)]
pub struct TodayArgs {
    /// 学年 4 位（如 `2025`）。留空 = 当前学年。
    #[arg(long)]
    pub xnm: Option<String>,
    /// 学期编码：`3`=秋季 / `12`=春季 / `16`=夏季。留空 = 当前学期。
    #[arg(long)]
    pub xqm: Option<String>,
    /// 用 1×N 网格形式输出（仅 TTY；非 TTY 时 fallback YAML）。
    #[arg(long, default_value_t = false)]
    pub grid: bool,
}

/// `sjtu jwc week [--zs N] [--xnm 2025] [--xqm 12] [--grid]`：整周课表。
#[derive(Debug, Args)]
pub struct WeekArgs {
    /// 学年 4 位。
    #[arg(long)]
    pub xnm: Option<String>,
    /// 学期编码。
    #[arg(long)]
    pub xqm: Option<String>,
    /// 教学周次 1..=18。留空 = 自动反推当前周。
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=18))]
    pub zs: Option<u8>,
    /// 用 7×N 网格形式输出。
    #[arg(long, default_value_t = false)]
    pub grid: bool,
}

/// `sjtu jwc next [--within N] [--limit K]`：接下来若干天的前 K 节课。
#[derive(Debug, Args)]
pub struct NextArgs {
    #[arg(long)]
    pub xnm: Option<String>,
    #[arg(long)]
    pub xqm: Option<String>,
    /// 时间窗口：未来 N 天内的课（1..=31）。默认 1（仅今天剩余）。
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=31), default_value_t = 1)]
    pub within: u8,
    /// 最多返回前 K 节课。默认 5。
    #[arg(long, default_value_t = 5)]
    pub limit: u8,
}
```

- [ ] **Step 6：跑测试确认 pass**

注：还需要确保 `src/lib.rs` 把 `cli` 模块 `pub mod cli;` 暴露，且 `src/cli/mod.rs` 的 `mod jwc;` 改 `pub mod jwc;`。如已 pub 跳过。

```bash
cargo test --test cli_jwc_schedule_clap 2>&1 | tail -10
```

注：此时 `cmd_today / cmd_week / cmd_next` 还没在 `commands::jwc` 里定义，dispatch 编译会 fail。**先 stub 3 个 dummy handler 占位**（在 T9 实装时替换）：

修改 `src/commands/jwc/mod.rs` 加 stub：

```rust
mod data;
mod handlers;
mod schedule_handlers;

pub use handlers::{cmd_exams, cmd_gpa, cmd_grades, cmd_schedule};
pub use schedule_handlers::{cmd_next, cmd_today, cmd_week};
```

新建 `src/commands/jwc/schedule_handlers.rs` 写 stub：

```rust
//! T1 衍生命令 handler（cmd_today / cmd_week / cmd_next）。
//!
//! **占位实现** — 真正逻辑在 T9 完成。这里仅返回 unimplemented! 让 cli 编译过。

use anyhow::Result;
use crate::output::OutputFormat;

pub async fn cmd_today(
    _xnm: Option<String>,
    _xqm: Option<String>,
    _grid: bool,
    _fmt: Option<OutputFormat>,
) -> Result<()> {
    unimplemented!("T9 will implement this")
}

pub async fn cmd_week(
    _xnm: Option<String>,
    _xqm: Option<String>,
    _zs: Option<u8>,
    _grid: bool,
    _fmt: Option<OutputFormat>,
) -> Result<()> {
    unimplemented!("T9 will implement this")
}

pub async fn cmd_next(
    _xnm: Option<String>,
    _xqm: Option<String>,
    _within: u8,
    _limit: u8,
    _fmt: Option<OutputFormat>,
) -> Result<()> {
    unimplemented!("T9 will implement this")
}
```

再跑：

```bash
cargo test --test cli_jwc_schedule_clap 2>&1 | tail -10
```

Expected: `test result: ok. 7 passed`。

- [ ] **Step 7：cargo fmt + clippy**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: 无 warning。

- [ ] **Step 8：行数检查**

```bash
wc -l src/cli/jwc/mod.rs src/cli/jwc/schedule_cli.rs src/commands/jwc/schedule_handlers.rs
```

Expected: 三个文件全部 ≤ 200 行。

- [ ] **Step 9：Commit**

```bash
git add src/cli/jwc/ src/commands/jwc/mod.rs src/commands/jwc/schedule_handlers.rs tests/cli_jwc_schedule_clap.rs
git rm src/cli/jwc.rs
git commit -m "refactor(cli): split jwc.rs into jwc/{mod,schedule_cli}.rs + add Today/Week/Next variants"
```

---

## Task T8：commands/jwc/data.rs 加 TodayData / WeekData / NextData

**Files:**
- Modify: `src/commands/jwc/data.rs`（71 行 → ~135 行）

- [ ] **Step 1：写 failing test**

在 `src/commands/jwc/data.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn today_data_serializes_with_all_fields() {
        let d = TodayData {
            xnm: Some("2025".into()),
            xqm: Some("12".into()),
            current_week: 14,
            today_iso: "2026-05-12".into(),
            today_weekday: 2,
            hint: None,
            items: vec![],
        };
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["current_week"], 14);
        assert_eq!(json["today_iso"], "2026-05-12");
    }

    #[test]
    fn week_data_omits_hint_when_none() {
        let d = WeekData {
            xnm: None,
            xqm: None,
            current_week: 14,
            query_zs: 14,
            rqazc_list: vec![],
            hint: None,
            items: vec![],
        };
        let s = serde_json::to_string(&d).unwrap();
        assert!(!s.contains("hint"), "hint=None 应不序列化");
    }

    #[test]
    fn next_data_includes_fetched_weeks_and_limit() {
        let d = NextData {
            xnm: None,
            xqm: None,
            current_week: 14,
            within_days: 7,
            limit: 5,
            fetched_weeks: vec![14, 15],
            hint: None,
            items: vec![],
        };
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["within_days"], 7);
        assert_eq!(json["fetched_weeks"], serde_json::json!([14, 15]));
    }
}
```

- [ ] **Step 2：跑测试确认 fail**

```bash
cargo test --lib commands::jwc::data::tests 2>&1 | tail -10
```

Expected: 编译 fail（3 个 struct 不存在）。

- [ ] **Step 3：实装 3 个 Data struct**

在 `src/commands/jwc/data.rs` 末尾（`ExamsData` 之后）追加：

```rust
use crate::apps::jwc::RqAzc;

/// `sjtu jwc today` data 形状。
#[derive(Debug, Serialize)]
pub(super) struct TodayData {
    pub xnm: Option<String>,
    pub xqm: Option<String>,
    pub current_week: u8,
    /// 今日 ISO 日期（"2026-05-12"）。
    pub today_iso: String,
    /// 今日 周几（1=周一 .. 7=周日）。
    pub today_weekday: u8,
    /// 提示文字（如 "学期未开始" / "学期已结束/假期"）；None 时不序列化。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// 已铺平的今日剩余课程（含时刻 / 教室）。
    pub items: Vec<TodayItem>,
}

/// `sjtu jwc week` data 形状。
#[derive(Debug, Serialize)]
pub(super) struct WeekData {
    pub xnm: Option<String>,
    pub xqm: Option<String>,
    pub current_week: u8,
    pub query_zs: u8,
    pub rqazc_list: Vec<RqAzc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    pub items: Vec<WeekItem>,
}

/// `sjtu jwc next` data 形状。
#[derive(Debug, Serialize)]
pub(super) struct NextData {
    pub xnm: Option<String>,
    pub xqm: Option<String>,
    pub current_week: u8,
    pub within_days: u8,
    pub limit: u8,
    pub fetched_weeks: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    pub items: Vec<NextItem>,
}

/// today / week 的单条课程（含时刻 + 节次展开）。
#[derive(Debug, Serialize)]
pub(super) struct TodayItem {
    pub kcmc: Option<String>,
    pub xqj: u8,
    pub jc_list: Vec<u8>,
    /// 与 jc_list 对齐的 (start, end) 时刻字符串（"08:00", "08:45"）。
    pub clock_list: Vec<(String, String)>,
    pub jcor_fallback: Option<String>,
    pub cdmc: Option<String>,
    pub xm: Option<String>,
    pub kch: Option<String>,
}

/// week 的单条课程（同 TodayItem 但语义对应整周；保持独立类型以便后续分歧）。
pub(super) type WeekItem = TodayItem;

/// next 的单条课程（含 absolute datetime）。
#[derive(Debug, Serialize)]
pub(super) struct NextItem {
    pub kcmc: Option<String>,
    pub datetime_start: String,
    pub datetime_end: String,
    pub week: u8,
    pub xqj: u8,
    pub jc_list: Vec<u8>,
    pub cdmc: Option<String>,
    pub xm: Option<String>,
}
```

- [ ] **Step 4：跑测试确认 pass**

```bash
cargo test --lib commands::jwc::data::tests 2>&1 | tail -10
```

Expected: `test result: ok. 3 passed`。

- [ ] **Step 5：cargo fmt + clippy**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: 无 warning。

- [ ] **Step 6：行数检查**

```bash
wc -l src/commands/jwc/data.rs
```

Expected: ≤ 200 行（实际约 ~150 行）。

- [ ] **Step 7：Commit**

```bash
git add src/commands/jwc/data.rs
git commit -m "feat(jwc): add TodayData/WeekData/NextData + TodayItem/NextItem envelopes"
```

---

## Task T9：schedule_handlers.rs 实装 cmd_today / cmd_week / cmd_next

**Files:**
- Modify: `src/commands/jwc/schedule_handlers.rs`（T7 stub → 完整实装）

- [ ] **Step 1：写 failing test**

在 `src/commands/jwc/schedule_handlers.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::jwc::{KbItem, Schedule, RqAzc};

    fn make_schedule(kb: Vec<KbItem>, week1: &str) -> Schedule {
        Schedule {
            xqjmc_map: serde_json::json!({}),
            kb_list: kb,
            rqazc_list: vec![
                RqAzc { rq: Some(week1.to_string()), xqj: Some("1".to_string()) },
            ],
        }
    }

    #[test]
    fn filter_by_week_drops_courses_not_in_current_week() {
        let kb = vec![
            // 第 2 周 = bit 1 set
            KbItem { kcmc: Some("A".into()), old_zc: Some(0b10), old_jc: Some(0b1100), xqj: Some("2".into()), ..Default::default() },
            // 仅第 1 周 = bit 0 set，第 14 周不在
            KbItem { kcmc: Some("B".into()), old_zc: Some(0b1), old_jc: Some(0b1100), xqj: Some("2".into()), ..Default::default() },
        ];
        let filtered = filter_kb_in_week(&kb, 14);
        assert!(filtered.iter().any(|k| k.kcmc.as_deref() == Some("A")));
        assert!(!filtered.iter().any(|k| k.kcmc.as_deref() == Some("B")));
    }

    #[test]
    fn weeks_to_fetch_within_1_returns_1_week() {
        assert_eq!(weeks_to_fetch_for_within(1), 1);
    }

    #[test]
    fn weeks_to_fetch_within_7_returns_2_weeks() {
        // 7 天可能跨周，保守拉 2 周
        assert_eq!(weeks_to_fetch_for_within(7), 2);
    }

    #[test]
    fn weeks_to_fetch_within_31_returns_5_weeks() {
        assert_eq!(weeks_to_fetch_for_within(31), 5);
    }
}
```

- [ ] **Step 2：跑测试确认 fail**

```bash
cargo test --lib commands::jwc::schedule_handlers::tests 2>&1 | tail -10
```

Expected: 编译 fail（`filter_kb_in_week` / `weeks_to_fetch_for_within` 不存在）。

- [ ] **Step 3：实装 cmd_today / cmd_week / cmd_next + helpers**

把 `src/commands/jwc/schedule_handlers.rs` 内容**整体替换**为：

```rust
//! `sjtu jwc today / week / next`：T1 衍生命令 handler。
//!
//! 数据流：
//! - 反推当前周（infer_current_week）→ 拉 N2154（schedule_by_week）→ 按 oldzc 过滤本周课
//! - 节次 bitmask（oldjc）展开 → join period_clock 得到时刻
//! - today: 仅今日 + 仅未来时刻；week: 整周铺平；next: 多周 + 排序 + take K

use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime};

use crate::apps::jwc::{period_clock, Client, KbItem};
use crate::output::{render, Envelope, OutputFormat};

use super::data::{NextData, NextItem, TodayData, TodayItem, WeekData};

/// `sjtu jwc today`：今日剩余的课。
pub async fn cmd_today(
    xnm: Option<String>,
    xqm: Option<String>,
    _grid: bool, // grid 渲染由 T10 单独承接；本 handler 先输出列表
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let client = Client::connect().await?;
    let cw = client
        .infer_current_week(xnm.as_deref(), xqm.as_deref())
        .await?;

    let today = Local::now().date_naive();
    let today_weekday = iso_weekday(today);
    let today_iso = today.format("%Y-%m-%d").to_string();

    if cw == 0 {
        return render(
            Envelope::ok(TodayData {
                xnm, xqm, current_week: 0, today_iso, today_weekday,
                hint: Some("学期未开始".into()), items: vec![],
            }),
            fmt,
        );
    }
    if cw > 18 {
        return render(
            Envelope::ok(TodayData {
                xnm, xqm, current_week: cw, today_iso, today_weekday,
                hint: Some("学期已结束 / 假期".into()), items: vec![],
            }),
            fmt,
        );
    }

    let sched = client.schedule_by_week(xnm.as_deref(), xqm.as_deref(), cw).await?;
    let filtered = filter_kb_in_week(&sched.kb_list, cw);
    let now_time = Local::now().time();

    let mut items: Vec<TodayItem> = Vec::new();
    for k in filtered.iter() {
        let xqj = parse_xqj(k.xqj.as_deref());
        if xqj != today_weekday {
            continue;
        }
        let (jc_list, clock_list) = expand_jc(k.old_jc);
        if clock_list.is_empty() {
            continue;
        }
        // 今日剩余：保留 end_time > now 的节
        let last_end_str = clock_list.last().map(|(_, e)| e.clone()).unwrap_or_default();
        if let Ok(last_end) = chrono::NaiveTime::parse_from_str(&last_end_str, "%H:%M") {
            if last_end <= now_time {
                continue;
            }
        }
        items.push(TodayItem {
            kcmc: k.kcmc.clone(),
            xqj: xqj as u8,
            jc_list,
            clock_list,
            jcor_fallback: k.jcor.clone(),
            cdmc: k.cdmc.clone(),
            xm: k.xm.clone(),
            kch: k.kch.clone(),
        });
    }
    items.sort_by_key(|i| i.jc_list.first().copied().unwrap_or(99));

    render(Envelope::ok(TodayData {
        xnm, xqm, current_week: cw, today_iso, today_weekday,
        hint: None, items,
    }), fmt)
}

/// `sjtu jwc week`：指定周（或当前周）整周课表。
pub async fn cmd_week(
    xnm: Option<String>,
    xqm: Option<String>,
    zs: Option<u8>,
    _grid: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let client = Client::connect().await?;
    let cw = client.infer_current_week(xnm.as_deref(), xqm.as_deref()).await?;
    let query_zs = zs.unwrap_or(cw.max(1).min(18));

    let sched = client.schedule_by_week(xnm.as_deref(), xqm.as_deref(), query_zs).await?;
    let filtered = filter_kb_in_week(&sched.kb_list, query_zs);

    let mut items: Vec<TodayItem> = Vec::new();
    for k in filtered.iter() {
        let xqj = parse_xqj(k.xqj.as_deref());
        let (jc_list, clock_list) = expand_jc(k.old_jc);
        if jc_list.is_empty() { continue; }
        items.push(TodayItem {
            kcmc: k.kcmc.clone(),
            xqj: xqj as u8,
            jc_list,
            clock_list,
            jcor_fallback: k.jcor.clone(),
            cdmc: k.cdmc.clone(),
            xm: k.xm.clone(),
            kch: k.kch.clone(),
        });
    }
    items.sort_by_key(|i| (i.xqj, i.jc_list.first().copied().unwrap_or(99)));

    let hint = if cw == 0 { Some("学期未开始".into()) }
        else if cw > 18 { Some("学期已结束 / 假期".into()) }
        else { None };

    render(Envelope::ok(WeekData {
        xnm, xqm, current_week: cw, query_zs,
        rqazc_list: sched.rqazc_list,
        hint, items,
    }), fmt)
}

/// `sjtu jwc next`：接下来 within 天内前 limit 节课。
pub async fn cmd_next(
    xnm: Option<String>,
    xqm: Option<String>,
    within: u8,
    limit: u8,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let client = Client::connect().await?;
    let cw = client.infer_current_week(xnm.as_deref(), xqm.as_deref()).await?;
    let now = Local::now().naive_local();
    let today = now.date();

    if cw == 0 {
        return render(Envelope::ok(NextData {
            xnm, xqm, current_week: 0, within_days: within, limit,
            fetched_weeks: vec![], hint: Some("学期未开始".into()), items: vec![],
        }), fmt);
    }
    if cw > 18 {
        return render(Envelope::ok(NextData {
            xnm, xqm, current_week: cw, within_days: within, limit,
            fetched_weeks: vec![], hint: Some("学期已结束 / 假期".into()), items: vec![],
        }), fmt);
    }

    let n_weeks = weeks_to_fetch_for_within(within);
    let mut fetched_weeks: Vec<u8> = Vec::new();
    let mut all_items: Vec<NextItem> = Vec::new();

    for offset in 0..n_weeks {
        let zs = cw.saturating_add(offset);
        if zs > 18 { break; }
        fetched_weeks.push(zs);

        let sched = client.schedule_by_week(xnm.as_deref(), xqm.as_deref(), zs).await?;
        // 计算这个 zs 周的周一日期：rqazc_list[0].rq 给出周一
        let week_mon = sched.rqazc_list.iter()
            .find_map(|r| r.rq.as_deref().filter(|_| r.xqj.as_deref() == Some("1")))
            .or_else(|| sched.rqazc_list.first().and_then(|r| r.rq.as_deref()))
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        let Some(week_mon) = week_mon else { continue };
        let filtered = filter_kb_in_week(&sched.kb_list, zs);

        for k in filtered.iter() {
            let xqj = parse_xqj(k.xqj.as_deref());
            if !(1..=7).contains(&xqj) { continue; }
            let (jc_list, clock_list) = expand_jc(k.old_jc);
            if jc_list.is_empty() { continue; }
            let course_date = week_mon + Duration::days((xqj - 1) as i64);
            if course_date < today { continue; }
            if (course_date - today).num_days() > within as i64 { continue; }

            let start_str = clock_list.first().map(|(s, _)| s.clone()).unwrap_or_default();
            let end_str = clock_list.last().map(|(_, e)| e.clone()).unwrap_or_default();
            let start_dt = combine_dt(course_date, &start_str);
            let end_dt = combine_dt(course_date, &end_str);
            if start_dt <= now && course_date == today { continue; }

            all_items.push(NextItem {
                kcmc: k.kcmc.clone(),
                datetime_start: start_dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
                datetime_end: end_dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
                week: zs,
                xqj: xqj as u8,
                jc_list,
                cdmc: k.cdmc.clone(),
                xm: k.xm.clone(),
            });
        }
    }

    all_items.sort_by(|a, b| a.datetime_start.cmp(&b.datetime_start));
    all_items.truncate(limit as usize);

    render(Envelope::ok(NextData {
        xnm, xqm, current_week: cw, within_days: within, limit,
        fetched_weeks, hint: None, items: all_items,
    }), fmt)
}

// ============ helpers ============

/// 过滤 kb_list 仅保留当前周课。
pub(crate) fn filter_kb_in_week<'a>(kb: &'a [KbItem], week: u8) -> Vec<&'a KbItem> {
    kb.iter()
        .filter(|k| match k.old_zc {
            Some(z) => period_clock::is_in_week(z, week),
            None => true, // 缺 bitmask → 保守保留（envelope.meta 可加 fallback flag）
        })
        .collect()
}

/// within → 需要拉的周数。1..=7 → 2（保守跨周）；8..=14 → 3；以此类推；上限 5。
pub(crate) fn weeks_to_fetch_for_within(within: u8) -> u8 {
    match within {
        0..=1 => 1,
        2..=7 => 2,
        8..=14 => 3,
        15..=21 => 4,
        _ => 5,
    }
}

/// 展开 `old_jc` bitmask → 节次列表 + 与之对齐的 (start, end) 字符串列表。
fn expand_jc(old_jc: Option<u32>) -> (Vec<u8>, Vec<(String, String)>) {
    let Some(jc) = old_jc else { return (vec![], vec![]) };
    let jcs = period_clock::jc_positions(jc);
    let clocks: Vec<(String, String)> = jcs.iter().filter_map(|j| {
        let (s, e) = period_clock::lookup(*j)?;
        Some((s.format("%H:%M").to_string(), e.format("%H:%M").to_string()))
    }).collect();
    (jcs, clocks)
}

/// "1".."7" → u8；非法 → 0。
fn parse_xqj(s: Option<&str>) -> u8 {
    s.and_then(|x| x.parse::<u8>().ok()).filter(|n| *n >= 1 && *n <= 7).unwrap_or(0)
}

/// 日期 + "HH:MM" → NaiveDateTime（解析失败 → 当天 00:00）。
fn combine_dt(d: NaiveDate, hhmm: &str) -> NaiveDateTime {
    let t = chrono::NaiveTime::parse_from_str(hhmm, "%H:%M")
        .unwrap_or_else(|_| chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    d.and_time(t)
}

/// today → ISO weekday u8（1=Mon..7=Sun）。chrono `weekday()` 给的也是 Mon 起。
fn iso_weekday(d: NaiveDate) -> u8 {
    d.weekday().number_from_monday() as u8
}
```

- [ ] **Step 4：跑测试确认 pass**

```bash
cargo test --lib commands::jwc::schedule_handlers::tests 2>&1 | tail -10
```

Expected: `test result: ok. 4 passed`。

- [ ] **Step 5：cargo build 全量检查**

```bash
cargo build 2>&1 | tail -10
```

Expected: `Finished dev [unoptimized + debuginfo]`，无 error。

- [ ] **Step 6：cargo fmt + clippy**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | tail -10
```

Expected: 无 warning。

- [ ] **Step 7：行数检查**

```bash
wc -l src/commands/jwc/schedule_handlers.rs
```

Expected: ≤ 200 行。若超 → 拆 helpers 到 `src/commands/jwc/schedule_render.rs`（plan 阶段未必到此地步，但若超提示拆）。

- [ ] **Step 8：Commit**

```bash
git add src/commands/jwc/schedule_handlers.rs
git commit -m "feat(jwc): implement cmd_today/cmd_week/cmd_next + bitmask filter + period clock join"
```

---

## Task T10：output/grid.rs 新建（comfy-table 渲染）

**Files:**
- Create: `src/output/grid.rs`
- Modify: `src/output.rs` 拆 → `src/output/mod.rs`（必要时；若 src/output.rs 现存才拆）
- 或：保持 `src/output.rs` 为模块文件，把 grid 作为兄弟模块 `src/output_grid.rs` 引入。**简化路径**：直接 `src/output_grid.rs`。

> **决策**：避免改 `src/output.rs` 现有 layout（一旦把文件 → 目录会触发 `mod grid;` 重组），用兄弟模块 `src/output_grid.rs` 引入。`src/lib.rs` 加 `pub mod output_grid;` 即可。

修正 Files：

- Create: `src/output_grid.rs`
- Modify: `src/lib.rs` 加 `pub mod output_grid;`

- [ ] **Step 1：写 failing test**

创建 `src/output_grid.rs`，只写 test：

```rust
//! 课表 grid 渲染（comfy-table 包装），仅 TTY 友好；非 TTY 走 YAML/JSON 输出。

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_grid_day_with_two_courses_outputs_non_empty_string() {
        let items = vec![
            DayCell { jc_list: vec![1, 2], kcmc: "高数".into(), cdmc: "东上 101".into(), xm: "张".into() },
            DayCell { jc_list: vec![3, 4], kcmc: "英语".into(), cdmc: "西中 202".into(), xm: "李".into() },
        ];
        let out = render_grid_day(&items);
        assert!(out.contains("高数"));
        assert!(out.contains("英语"));
        assert!(out.contains("1-2") || out.contains("1, 2"));
    }

    #[test]
    fn render_grid_week_with_empty_items_outputs_header_only() {
        let dates = vec![
            ("周一".into(), "2026-05-11".into()),
            ("周二".into(), "2026-05-12".into()),
        ];
        let out = render_grid_week(&dates, &[]);
        assert!(out.contains("周一"));
        assert!(out.contains("周二"));
    }
}
```

- [ ] **Step 2：跑测试确认 fail**

```bash
cargo test --lib output_grid::tests 2>&1 | tail -10
```

Expected: 编译 fail（`DayCell` / `render_grid_day` / `render_grid_week` 不存在）。

- [ ] **Step 3：实装 grid 渲染**

把 `src/output_grid.rs` 替换为：

```rust
//! 课表 grid 渲染（comfy-table 包装）。
//!
//! - `render_grid_day(items)`：单日 1 列 N 行（节次×课程）
//! - `render_grid_week(week_dates, items)`：整周 7 列 × N 行（周一..周日）
//!
//! 终端窄度自适应：comfy-table `ContentArrangement::Dynamic` 自动 wrap 内容。

use comfy_table::{ContentArrangement, Table};

/// 单日 grid 的一格内容。
pub struct DayCell {
    pub jc_list: Vec<u8>,
    pub kcmc: String,
    pub cdmc: String,
    pub xm: String,
}

/// 整周 grid 的一格内容（含周几）。
pub struct WeekCell {
    pub xqj: u8, // 1..=7
    pub jc_list: Vec<u8>,
    pub kcmc: String,
    pub cdmc: String,
    pub xm: String,
}

/// 渲染单日表格。
pub fn render_grid_day(items: &[DayCell]) -> String {
    let mut t = Table::new();
    t.set_content_arrangement(ContentArrangement::Dynamic);
    t.set_header(vec!["节次", "课程", "教室", "教师"]);
    for c in items {
        t.add_row(vec![
            jc_range(&c.jc_list),
            c.kcmc.clone(),
            c.cdmc.clone(),
            c.xm.clone(),
        ]);
    }
    t.to_string()
}

/// 渲染整周表格。week_dates = [(周一文本, ISO 日期), ...] 长度 7。
pub fn render_grid_week(week_dates: &[(String, String)], items: &[WeekCell]) -> String {
    let mut t = Table::new();
    t.set_content_arrangement(ContentArrangement::Dynamic);

    // header: ["节次", "周一\n2026-05-11", "周二\n2026-05-12", ...]
    let mut header: Vec<String> = vec!["节次".to_string()];
    for (label, date) in week_dates {
        header.push(format!("{label}\n{date}"));
    }
    t.set_header(header);

    // 收集所有节次（取并集）
    let mut all_jc: Vec<u8> = items
        .iter()
        .flat_map(|c| c.jc_list.iter().copied())
        .collect();
    all_jc.sort_unstable();
    all_jc.dedup();

    for jc in all_jc {
        let mut row: Vec<String> = vec![jc.to_string()];
        for xqj in 1u8..=7 {
            let cell = items
                .iter()
                .find(|c| c.xqj == xqj && c.jc_list.contains(&jc))
                .map(|c| format!("{}\n{}\n{}", c.kcmc, c.cdmc, c.xm))
                .unwrap_or_default();
            row.push(cell);
        }
        t.add_row(row);
    }
    t.to_string()
}

/// "1, 2, 3" 简写成 "1-3"，不连续则保持逗号分隔。
fn jc_range(jcs: &[u8]) -> String {
    if jcs.is_empty() {
        return String::new();
    }
    let mut s: Vec<u8> = jcs.to_vec();
    s.sort_unstable();
    // 检查是否连续
    let consecutive = s.windows(2).all(|w| w[1] == w[0] + 1);
    if consecutive && s.len() > 1 {
        format!("{}-{}", s.first().unwrap(), s.last().unwrap())
    } else {
        s.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")
    }
}
```

- [ ] **Step 4：在 src/lib.rs 暴露模块**

`src/lib.rs` 加（如果还没暴露）：

```rust
pub mod output_grid;
```

放在现有 `pub mod output;` 行之后。

- [ ] **Step 5：跑测试确认 pass**

```bash
cargo test --lib output_grid::tests 2>&1 | tail -10
```

Expected: `test result: ok. 2 passed`。

- [ ] **Step 6：cargo fmt + clippy**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: 无 warning。

- [ ] **Step 7：行数检查**

```bash
wc -l src/output_grid.rs
```

Expected: ≤ 130 行。

- [ ] **Step 8：Commit**

```bash
git add src/output_grid.rs src/lib.rs
git commit -m "feat(output): add grid renderer (comfy-table dynamic, day + week)"
```

> **注**：本 task 不接入 `cmd_today/cmd_week` 的 `--grid` flag —— 此处 grid 输出尚是独立 helper。T13 收尾时把 `--grid` 标志接入 handler（在 grid 模式下用 `render_grid_*` 替换 envelope YAML 输出）。也可在 T9 内集成；为减少 T9 行数压力，本 plan 把 `--grid` 真接入留到 T13。

---

## Task T11：集成测试（mockito + fixture）

**Files:**
- Create: `tests/jwc_n2154_integration.rs`

> **前置**：T0 已经把 `tests/fixtures/jwc/n2154_week_zs1.json` + `n2154_week_zs14.json` 落盘。

- [ ] **Step 1：写集成测**

```rust
//! N2154 周次课表端点集成测（mockito 起本地 HTTP，用真机抓的脱敏 fixture）。
//!
//! 注意：当前测试覆盖的是"如果 N2154 响应是这个 JSON，CLI 拼装的 Envelope 长什么样"。
//! 不测真实 SJTU session / CAS 链路（那是 #[ignore] 真机 smoke test）。

use std::fs;

#[test]
fn fixture_n2154_zs1_can_be_parsed_as_schedule() {
    let raw = fs::read_to_string("tests/fixtures/jwc/n2154_week_zs1.json")
        .expect("fixture n2154_week_zs1.json 必须存在（T0 抓）");
    let s: sjtu_cli::apps::jwc::Schedule =
        serde_json::from_str(&raw).expect("Schedule 解析失败");
    // T0 抓的真实 zs=1 响应必带 rqazcList（第 1 周 7 天）
    assert!(
        !s.rqazc_list.is_empty(),
        "zs=1 响应必带 rqazcList（用于反推今天周次）"
    );
    // 第一天应该是第 1 周周一
    assert_eq!(s.rqazc_list[0].xqj.as_deref(), Some("1"));
}

#[test]
fn fixture_n2154_zs14_has_kb_list_with_old_zc_bits_for_week_14() {
    let raw = fs::read_to_string("tests/fixtures/jwc/n2154_week_zs14.json")
        .expect("fixture n2154_week_zs14.json 必须存在（T0 抓）");
    let s: sjtu_cli::apps::jwc::Schedule =
        serde_json::from_str(&raw).expect("Schedule 解析失败");
    // 至少一条课的 oldzc 在第 14 周（位 13）有 1
    let any_in_week14 = s.kb_list.iter().any(|k| {
        k.old_zc
            .map(|z| sjtu_cli::apps::jwc::period_clock::is_in_week(z, 14))
            .unwrap_or(false)
    });
    assert!(
        any_in_week14,
        "zs=14 响应应至少有 1 节课的 oldzc 在第 14 周（否则 fixture 不对）"
    );
}
```

**注意**：上面 `sjtu_cli::apps::jwc::period_clock::is_in_week` 需要 `period_clock` 在 `apps/jwc/mod.rs` 里 `pub mod`（T4 已 pub）+ `lib.rs` 已 `pub mod apps`。检查 `src/apps/mod.rs` 的 `pub mod jwc;` 是否存在。如有 `mod jwc;`（非 pub）则改 `pub mod`。

- [ ] **Step 2：跑测试**

```bash
cargo test --test jwc_n2154_integration 2>&1 | tail -10
```

Expected: 2 个 test 都 pass（如 T0 fixture 没抓 → fail，回去补 T0）。

- [ ] **Step 3：cargo fmt + clippy**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: 无 warning。

- [ ] **Step 4：Commit**

```bash
git add tests/jwc_n2154_integration.rs
git commit -m "test(jwc): integration test for N2154 fixtures (zs=1 rqazc + zs=14 oldzc bitmask)"
```

---

## Task T12：真机 smoke 测 today / week / next

> **必须主对话亲跑**：需要 `sjtu login` + 真实 SJTU session。

**Files:**（只跑命令，不改文件）

- [ ] **Step 1：确认 session 在线**

```bash
cargo run --release -- status
```

Expected: `ok: true` + 显示 session TTL。若 expired → `cargo run --release -- login`。

- [ ] **Step 2：cmd_today smoke**

```bash
cargo run --release -- jwc today --yaml 2>&1 | head -40
```

Expected: Envelope `ok: true`，`current_week` 是合理值（5 月 12 日推算 = 第 12-15 周左右），`today_iso = 2026-05-12`，items 列表（可能为空 = 今天没课或都过期）。

- [ ] **Step 3：cmd_week smoke**

```bash
cargo run --release -- jwc week --yaml 2>&1 | head -60
```

Expected: `rqazc_list` 有 7 天 ISO，items 有当周课程（按 oldzc 已过滤）。

- [ ] **Step 4：cmd_week --zs 1 smoke**

```bash
cargo run --release -- jwc week --zs 1 --yaml 2>&1 | head -40
```

Expected: rqazc_list[0].rq 是开学第 1 周周一（如 2025-09-08）。

- [ ] **Step 5：cmd_next smoke**

```bash
cargo run --release -- jwc next --within 7 --limit 10 --yaml 2>&1 | head -80
```

Expected: items 按 `datetime_start` 排序，前 10 条（或 ≤ 10 条若周末无课），`fetched_weeks` 至少含当前周。

- [ ] **Step 6：cmd_next --within 31 性能 smoke**

```bash
time cargo run --release -- jwc next --within 31 --limit 30 --yaml 2>&1 | tail -5
```

Expected: 总耗时 ≤ 5s（5 次 N2154 + throttle = ~3s + 真实 RTT）。

- [ ] **Step 7：观察 cache 文件**

```bash
ls -la ~/.cache/sjtu-cli/jwc_week_cache.json 2>/dev/null || ls "$LOCALAPPDATA/sjtu/sjtu-cli/cache/jwc_week_cache.json" 2>/dev/null
cat $(ls ~/.cache/sjtu-cli/jwc_week_cache.json 2>/dev/null || echo "$LOCALAPPDATA/sjtu/sjtu-cli/cache/jwc_week_cache.json")
```

Expected: 文件存在，内含 `{"entries":{"__current__":{"week":..., "fetched_at": "..."}}}`。

- [ ] **Step 8：第二次调用走 cache，应该明显快**

```bash
time cargo run --release -- jwc today --yaml > /dev/null
```

Expected: 比第一次快至少 500ms（省了 zs=1 反推那一次 N2154）。

- [ ] **Step 9：如果以上任何步骤失败 → 在 tasks/lessons.md 记真机问题 + 不 commit**（lessons 在 T13 commit）

---

## Task T13：README / lessons.md / todo.md 收尾 + `--grid` flag 接入

> **主对话亲跑**：需要跨文档判断 + 把 `--grid` 真接入 handler（依赖 T9 + T10 都已 OK）。

**Files:**
- Modify: `README.md`
- Modify: `tasks/lessons.md`
- Modify: `tasks/todo.md`
- Modify: `src/commands/jwc/schedule_handlers.rs`（把 `_grid` 改成真接入 grid 渲染）

- [ ] **Step 1：把 `--grid` 接入 cmd_today / cmd_week**

修改 `src/commands/jwc/schedule_handlers.rs`：

1.1 在文件顶部 imports 加：

```rust
use crate::output_grid::{render_grid_day, render_grid_week, DayCell, WeekCell};
```

1.2 `cmd_today` 函数末尾改：把 `_grid` 改为 `grid`，并在 render 前判断：

```rust
    // ...构造 items 之后...
    if grid {
        let cells: Vec<DayCell> = items.iter().map(|i| DayCell {
            jc_list: i.jc_list.clone(),
            kcmc: i.kcmc.clone().unwrap_or_default(),
            cdmc: i.cdmc.clone().unwrap_or_default(),
            xm: i.xm.clone().unwrap_or_default(),
        }).collect();
        print!("{}", render_grid_day(&cells));
        return Ok(());
    }
    render(Envelope::ok(TodayData { /* ... */ }), fmt)
```

1.3 `cmd_week` 同理：

```rust
    if grid {
        let dates: Vec<(String, String)> = sched.rqazc_list.iter().enumerate().map(|(i, r)| {
            let label = ["周一","周二","周三","周四","周五","周六","周日"]
                .get(i).copied().unwrap_or("").to_string();
            (label, r.rq.clone().unwrap_or_default())
        }).collect();
        let cells: Vec<WeekCell> = items.iter().map(|i| WeekCell {
            xqj: i.xqj,
            jc_list: i.jc_list.clone(),
            kcmc: i.kcmc.clone().unwrap_or_default(),
            cdmc: i.cdmc.clone().unwrap_or_default(),
            xm: i.xm.clone().unwrap_or_default(),
        }).collect();
        print!("{}", render_grid_week(&dates, &cells));
        return Ok(());
    }
    render(Envelope::ok(WeekData { /* ... */ }), fmt)
```

注意：因为 grid 输出绕开了 envelope，**非 TTY 时仍走 YAML**——可以加一个 TTY 检测，非 TTY 时 ignore grid flag。简单做法：直接尊重用户的 `--grid`，让用户用 `--yaml` 来覆盖（互斥 clap 检查可后续加）。

- [ ] **Step 2：手动测 grid**

```bash
cargo run --release -- jwc today --grid 2>&1 | head -30
cargo run --release -- jwc week --grid 2>&1 | head -30
```

Expected: 输出 comfy-table 表格，肉眼可读。

- [ ] **Step 3：更新 README.md**

修改 `README.md` 第 27 行（`sjtu jwc grades` 那一行）变成：

```markdown
| `sjtu jwc grades\|schedule\|gpa\|exams\|today\|week\|next` | 教务（i.sjtu.edu.cn）—— N305005 成绩 / N2151 学年学期课表 / N309131 GPA / N358105 考试 / N2154 衍生（今日 / 整周 / 接下来 N 天）；`--grid` 表格输出 |
```

并在快速开始 section 末尾加：

```markdown
教务课表：

```bash
sjtu jwc today --grid                       # 今日剩余的课（comfy-table）
sjtu jwc week --zs 14 --grid                # 第 14 周整周表格
sjtu jwc next --within 7 --limit 10 --yaml  # 未来 7 天前 10 节课
```
```

- [ ] **Step 4：更新 tasks/lessons.md**

在 lessons.md 末尾追加（精确日期、内容）：

```markdown
## T1 jwc schedule derivatives（2026-05-12）

### N2154 周次 bitmask + 反推今天周次（高复用范式）
**坑**：ZF N2154 给的 `oldzc` 是按位 mask（位 0 = 第 1 周），不是字符串 "1-18周"。`zcd` 字符串只是人类可读 fallback，**bitmask 才是精确数据源**。
**正解**：`(old_zc >> (week - 1)) & 1` 判周次；`old_jc` 同理判节次。`jcor` "3-4" 也只是 fallback，多节不连续时会失真。
**反推今天周次**：调一次 N2154 zs=1 → rqazcList[0].rq 给第 1 周周一 ISO → 今日 - 周一 → / 7 + 1。**零额外端点**。

### XDG cache_dir 分离原则
**坑**：早期一切塞 `~/.config/sjtu-cli/`，混了凭证 + 临时 cache。系统级 cache 清理工具会要么不动它（错过清理），要么误删 session。
**正解**：`directories::ProjectDirs::cache_dir()`，Linux/macOS/Windows 各按 OS 标准走 `~/.cache/` / `~/Library/Caches/` / `%LOCALAPPDATA%`。

### comfy-table MSRV 锁版
**坑**：`comfy-table = "7"` 在 cargo 自动选 patch 时会跳到 7.2，触发 MSRV 1.85，超项目 rust-version 1.75。
**正解**：写 `comfy-table = "~7.1"` 锁 7.1.x（MSRV 1.64 兼容）。**联网验证 MSRV 是引入新依赖前必做项**。

### clap 嵌套模块拆分
**约定**：当 cli/<sub>.rs 接近 150 行，拆成 cli/<sub>/{mod.rs, <feature>_cli.rs}。`Args struct` 走 `#[derive(Args)]` 拆到 feature 文件；`Subcommand enum` 留在 mod.rs 当模块入口。
```

- [ ] **Step 5：更新 tasks/todo.md**

把 T1 标完成 + 列出 T2..T11 的 next steps。具体行号需打开 todo.md 看，通常加：

```markdown
- [x] T1 jwc 课表查询 today/week/next（2026-05-12 完成）
  - N2154 周次端点 + oldzc/oldjc bitmask 过滤
  - infer_current_week + cache 分离（cache_dir）
  - period_clock 1-13 节 fallback 表（T0 调研后回填）
  - comfy-table grid 渲染（--grid）
  - 13 task 全部通过：7 unit + 4 schedule_handlers + 2 integration + 真机 smoke
```

- [ ] **Step 6：cargo fmt + clippy + 全量 test**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test --lib 2>&1 | tail -15
```

Expected: 无 warning + 全部 test pass。

- [ ] **Step 7：Commit 收尾**

```bash
git add README.md tasks/lessons.md tasks/todo.md src/commands/jwc/schedule_handlers.rs
git commit -m "feat(jwc): wire --grid into cmd_today/cmd_week + docs (README/lessons/todo)"
```

- [ ] **Step 8：最终 review**

```bash
git log --oneline -15
cargo test --lib 2>&1 | grep -E "test result|FAILED"
wc -l src/cli/jwc/mod.rs src/cli/jwc/schedule_cli.rs src/commands/jwc/schedule_handlers.rs src/apps/jwc/api/schedule.rs src/apps/jwc/period_clock.rs src/output_grid.rs
```

Expected：
- 13 个 commit 整齐
- 全部 test pass
- 所有新建/修改文件 < 200 行

---

## Self-Review（plan 写完后内嵌做的检查）

### 1. Spec 覆盖

| Spec 要求 | Plan task |
|---|---|
| G1 cmd_today 今日剩余 | T9 |
| G2 cmd_week --zs N 整周 | T9 |
| G3 cmd_next --within --limit | T9 |
| G4 --grid 表格输出 | T10 + T13 接入 |
| G5 反推当前周 + 24h cache | T6 |
| G6 节次 → 时刻映射 | T0 + T4 |
| §6.1 period_clock.rs 新建 | T4 |
| §6.1 schedule_handlers.rs 新建 | T9 |
| §6.1 cli/jwc/{mod,schedule_cli}.rs | T7 |
| §6.1 output/grid.rs 新建 | T10（实际改 output_grid.rs 兄弟模块）|
| §6.2 config.rs +cache_dir | T2 |
| §6.2 api/schedule.rs +schedule_by_week +infer_current_week | T5 + T6 |
| §6.2 models/schedule.rs +oldzc/oldjc/rqazc_list | T3 |
| §6.2 commands/jwc/data.rs +TodayData/WeekData/NextData | T8 |
| §6.2 Cargo.toml +comfy-table | T1 |
| §7.1 周次反推算法 | T6 单测 + T12 真机 |
| §7.2 oldzc/oldjc bitmask | T4 单测 |
| §7.3 节次时刻 | T0 调研 + T4 fallback |
| §7.4 next within=31 → 5 周 | T9 weeks_to_fetch_for_within + T12 性能测 |
| §8 Envelope shape | T8 + T9 |
| §9 Error handling 矩阵 | T9 (hint) + 现有 cas_login 链 |
| §10 Testing 单元/集成/真机 | T3-T10 单元 + T11 集成 + T12 真机 |
| §11.3 跨平台路径 | T2 + T12 验证 |
| §13 D1-D7 决策落地 | 全部 |

✅ 全覆盖。

### 2. Placeholder scan

- T0 step 5 提到"等 T4 实装时抄"——这是 cross-task ref，不算 placeholder（明确告诉 subagent 去哪查）。
- T4 step 3 `DEFAULT_TABLE` 注释提到"T0 调研未完成时这是占位值"——T0 主对话亲跑会先于 T4 完成；若 T0 真发现实际时刻与表不同，T0 step 5 会回填精确值。**不是 plan 缺陷**。
- 其余 task：每个 step 都有完整 commands + 代码 + expected。✅

### 3. Type consistency

- `KbItem.old_zc: Option<u32>` / `KbItem.old_jc: Option<u32>` —— T3 定义、T9 使用、T11 集成测验证 ✅
- `Schedule.rqazc_list: Vec<RqAzc>` —— T3 定义、T6 反推用、T9 cmd_week 透传 ✅
- `period_clock::lookup(jc: u8) -> Option<(NaiveTime, NaiveTime)>` —— T4 定义、T9 expand_jc 使用 ✅
- `cache_key(xnm, xqm) -> String` / `cache_ttl_seconds` —— T6 定义、内部使用 ✅
- `weeks_to_fetch_for_within(within: u8) -> u8` —— T9 定义 + 单测 ✅
- `TodayItem / WeekItem (type alias) / NextItem` —— T8 定义、T9 构造 ✅
- `DayCell / WeekCell` —— T10 定义、T13 (cmd_today/cmd_week grid 分支) 使用 ✅

✅ 所有跨 task 类型 / 函数名匹配。

---

**Plan 完成时间预估**：

- T0：1h（主对话 chrome-devtools 调研 + 2 个 fixture 抓取）
- T1-T11：subagent 并行 10-15 min/task → ~2h（含 review 循环）
- T12：30 min（真机 smoke 6 个步骤）
- T13：30 min（docs + grid 接入）

**总计** ≈ 4h 主对话 + subagent 时间。
