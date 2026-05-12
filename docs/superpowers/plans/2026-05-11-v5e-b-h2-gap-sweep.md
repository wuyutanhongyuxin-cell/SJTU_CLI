# V5.E-B Implementation Plan — H2 multiplex + gap_threshold real-machine sweep

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 单讲 audio-only 下载墙钟 6.5 min → < 2 min（5-10× 加速），sustained 跨 9 讲；网络 705 MB → ≤ 500 MB；关 task #42。

**Architecture:** 启用 reqwest http2 feature → 撤 audio_dl 生产 client 的 `http1_only()` + `pool_max_idle_per_host(0)`，让 ALPN 自动协商 H2 → 单 TCP × N streams 多路复用替代 8 × 独立 TCP。同时 gap_threshold 走 env override 跑真机 sweep 找最优值，最后落硬编码。fail-soft 不动；`SJTU_FORCE_HTTP1=1` 兜底一键回退 V5.D。

**Tech Stack:** Rust stable / reqwest 0.12 + http2 feature / tokio / mockito（单测）/ 主对话亲跑真机（subagent 无 SJTU session）

**Spec:** `docs/superpowers/specs/2026-05-11-v5e-b-h2-gap-sweep-design.md`

**Subagent / 主对话 分工：**
- T0 / T1 / T2 / T5 / T7：subagent（mechanical，单测可覆盖）
- T3 / T4 / T6：**主对话亲跑**（subagent 没有 SJTU JAccount session）

---

## File Structure (touched files)

**Create:**
- `tmp/v5e_smoke/`（T3 产物目录）
- `tmp/v5e_sweep/sweep_results.md`（T4 产物）
- `tmp/v5e_phase2/_comparison.md`（T6 产物）

**Modify:**
- `Cargo.toml` ✅ 已加 `"http2"` feature（T0 验证）
- `src/apps/canvas_video/audio_dl/client.rs` — 撤 http1_only + 加 keepalive + 加 SJTU_FORCE_HTTP1 兜底
- `src/apps/canvas_video/audio_dl/orchestrator.rs` — RANGE_GAP_THRESHOLD 走 env override（T2）+ sweep 最优值替换（T5）
- `src/apps/canvas_video/audio_dl/tests.rs` — 加 env override 3 个新单测（T2）
- `CLAUDE.md` — 当前阶段段落（T7）
- `tasks/lessons.md` — 加 V5.E-B 新条目（T7）

**Test:**
- `src/apps/canvas_video/audio_dl/tests.rs` 现有 mockito 测全部应继续绿（撤 http1_only 后 mockito 仍走 HTTP/1.1，因 test_helpers.rs 仍是 http1_only）

---

## Task 0: 验证 Cargo.toml http2 feature 已就位 + 跑现有测全绿 baseline（subagent，5 min）

**Files:**
- Verify: `Cargo.toml` line 42 含 `"http2"`
- Run: `cargo check && cargo test --lib canvas_video::audio_dl`

- [ ] **Step 1：grep 验证 http2 feature 在 Cargo.toml**
```bash
grep '"http2"' Cargo.toml
```
Expected: `features = ["cookies", "json", "rustls-tls", "gzip", "http2"]`

- [ ] **Step 2：cargo check baseline 不挂**
```bash
cargo check
```
Expected: exit 0（已就位则只编 http2 feature 多出的 h2/hpack crate）

- [ ] **Step 3：现有 audio_dl 测试全绿（baseline）**
```bash
cargo test --lib canvas_video::audio_dl 2>&1 | tail -20
```
Expected: `test result: ok. N passed; 0 failed`

- [ ] **Step 4：记 N（baseline 测试数）到 task 注释**

---

## Task 1: 撤 audio_dl 生产 client http1_only + 加 H2 keep-alive + SJTU_FORCE_HTTP1 兜底（subagent，30 min）

**Files:**
- Modify: `src/apps/canvas_video/audio_dl/client.rs`
- Test: `src/apps/canvas_video/audio_dl/tests.rs`（已存在）

- [ ] **Step 1：写 failing test — SJTU_FORCE_HTTP1=1 env 时 client 走 HTTP/1.1（mockito 路径已 H1.1，验证 env 不破坏行为）**
```rust
// 在 tests.rs 末尾追加
#[tokio::test]
async fn build_client_audio_respects_force_http1_env() {
    // 简化：只断言 builder 不 panic + 返 Ok
    std::env::set_var("SJTU_FORCE_HTTP1", "1");
    let c = super::client::build_client_audio("https://example.com").unwrap();
    drop(c);
    std::env::remove_var("SJTU_FORCE_HTTP1");
}
```
**注**：reqwest 没暴露 H1/H2 配置 inspection API，单测只能验 builder 不挂；真 H2 协商靠 T3 真机 smoke 验。

- [ ] **Step 2：跑 test 确认 fail（因为 build_client_audio 还没读 env）**
```bash
cargo test --lib canvas_video::audio_dl::tests::build_client_audio_respects_force_http1_env
```
Expected: 编译挂或 PASS（因为新 test 只 build_client_audio 不挂；若已 PASS 跳到 Step 3）。

- [ ] **Step 3：实装 client.rs 改动**

Replace `src/apps/canvas_video/audio_dl/client.rs` 第 18-42 行 `build_client_audio` 函数体：

```rust
pub(super) fn build_client_audio(referer: &str) -> Result<Client> {
    if !referer.is_ascii() {
        return Err(
            SjtuCliError::InvalidInput(format!("Referer 含非 ASCII 字符：{referer}")).into(),
        );
    }
    let mut h = HeaderMap::new();
    h.insert(
        REFERER,
        HeaderValue::from_str(referer)
            .map_err(|e| SjtuCliError::InvalidInput(format!("Referer 无效: {e}")))?,
    );
    h.insert(USER_AGENT, HeaderValue::from_static(UA));

    // V5.E-B 启用 HTTP/2：撤 V3.1 时代 http1_only + pool_max_idle_per_host(0)，让 ALPN 自动
    // 协商 H2（probe_h2 实测 SJTU CDN 主动给 H2.0/200/RTT 1021 ms）。1 TCP × N streams 多路复用
    // 替代 8 × 独立 TCP，降 RTT 群从 150 → 10。
    //
    // 兜底：SJTU_FORCE_HTTP1=1 一键回退 V5.D 行为（H2 真机异常时排查）。
    let mut builder = Client::builder()
        .default_headers(h)
        .tcp_nodelay(true)
        .tcp_keepalive(Duration::from_secs(60))
        .timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(15));
    if std::env::var("SJTU_FORCE_HTTP1").as_deref() == Ok("1") {
        builder = builder.http1_only().pool_max_idle_per_host(0);
    }
    builder
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("audio_dl client: {e}")).into())
}
```

更新 module doc comment（line 1-2）：
```rust
//! audio_dl 专属 reqwest Client：V5.E-B 启用 H2 ALPN 多路复用 + 90 s 段级 timeout。
//! SJTU_FORCE_HTTP1=1 兜底一键回退 V5.D HTTP/1.1 行为。
```

- [ ] **Step 4：跑 test 确认 PASS**
```bash
cargo test --lib canvas_video::audio_dl
```
Expected: 全绿，新 test PASS。

- [ ] **Step 5：cargo fmt + clippy**
```bash
cargo fmt --check && cargo clippy --lib -- -D warnings
```
Expected: exit 0。

- [ ] **Step 6：wc -l 检查（client.rs ≤ 200 行）**
```bash
wc -l src/apps/canvas_video/audio_dl/client.rs
```
Expected: < 60 行（小改动）。

- [ ] **Step 7：commit**
```bash
git add Cargo.toml Cargo.lock src/apps/canvas_video/audio_dl/client.rs src/apps/canvas_video/audio_dl/tests.rs
git commit -m "feat(canvas-video): V5.E-B-T1 启用 H2 ALPN + SJTU_FORCE_HTTP1 兜底"
```

---

## Task 2: orchestrator.rs RANGE_GAP_THRESHOLD 走 env override（subagent，30 min）

**Files:**
- Modify: `src/apps/canvas_video/audio_dl/orchestrator.rs`
- Test: `src/apps/canvas_video/audio_dl/tests.rs`

- [ ] **Step 1：写 failing test —— 三个 env 行为**

```rust
// tests.rs 追加
#[test]
fn effective_gap_threshold_default_64kb() {
    std::env::remove_var("SJTU_GAP_THRESHOLD_KB");
    assert_eq!(super::orchestrator::effective_gap_threshold(), 64 * 1024);
}

#[test]
fn effective_gap_threshold_env_8kb() {
    std::env::set_var("SJTU_GAP_THRESHOLD_KB", "8");
    assert_eq!(super::orchestrator::effective_gap_threshold(), 8 * 1024);
    std::env::remove_var("SJTU_GAP_THRESHOLD_KB");
}

#[test]
fn effective_gap_threshold_env_invalid_falls_back() {
    std::env::set_var("SJTU_GAP_THRESHOLD_KB", "not-a-number");
    assert_eq!(super::orchestrator::effective_gap_threshold(), 64 * 1024);
    std::env::remove_var("SJTU_GAP_THRESHOLD_KB");
}
```

**注**：三测必须串行跑（env 是 process-wide），用 `cargo test -- --test-threads=1` 跑 env 测；或加 mutex。简化：先按串行，T4 真机 sweep 不依赖单测精确并发。

- [ ] **Step 2：跑 test 确认 fail（函数不存在 → 编译挂）**
```bash
cargo test --lib canvas_video::audio_dl::tests::effective_gap_threshold -- --test-threads=1
```
Expected: 编译 fail（`effective_gap_threshold` 不存在）。

- [ ] **Step 3：实装 orchestrator.rs**

替换 line 29 `const RANGE_GAP_THRESHOLD: u64 = 64 * 1024;`：

```rust
/// V5.D 真机基线值（详见前 doc 段）。V5.E-B real-machine sweep 后值（如有变更）覆盖。
pub(super) const RANGE_GAP_THRESHOLD_DEFAULT: u64 = 64 * 1024;

/// 读 SJTU_GAP_THRESHOLD_KB env (u32, KB → bytes)，invalid/unset → RANGE_GAP_THRESHOLD_DEFAULT。
/// V5.E-B 调研期专用，让真机 sweep 不需要重 build。
pub(super) fn effective_gap_threshold() -> u64 {
    std::env::var("SJTU_GAP_THRESHOLD_KB")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .map(|kb| (kb as u64) * 1024)
        .unwrap_or(RANGE_GAP_THRESHOLD_DEFAULT)
}
```

替换 line 59：
```rust
let ranges = super::ranges::merge_ranges(&samples, effective_gap_threshold());
```

加 info log 让 sweep 时看到 effective value：
```rust
let gap = effective_gap_threshold();
let ranges = super::ranges::merge_ranges(&samples, gap);
info!(
    range_count = ranges.len(),
    sample_count = samples.len(),
    gap_threshold = gap,
    "Range 合并完成"
);
```

- [ ] **Step 4：跑 test 确认 PASS**
```bash
cargo test --lib canvas_video::audio_dl -- --test-threads=1
```
Expected: 全绿。

- [ ] **Step 5：cargo fmt + clippy**
```bash
cargo fmt --check && cargo clippy --lib -- -D warnings
```

- [ ] **Step 6：wc -l 检查**
```bash
wc -l src/apps/canvas_video/audio_dl/orchestrator.rs
```
Expected: < 100 行。

- [ ] **Step 7：commit**
```bash
git add src/apps/canvas_video/audio_dl/orchestrator.rs src/apps/canvas_video/audio_dl/tests.rs
git commit -m "feat(canvas-video): V5.E-B-T2 RANGE_GAP_THRESHOLD 走 SJTU_GAP_THRESHOLD_KB env"
```

---

## Task 3: 单讲 H2 smoke 真机（主对话亲跑，15 min）

**Files:**
- Output: `tmp/v5e_smoke/L{N}_h2.stderr.log`, `tmp/v5e_smoke/L{N}_h2.envelope.json`

- [ ] **Step 1：拿 fresh URL（SJTU_PROBE_MULTIPART hook 临时插一次，验后 revert）**

或直接拿 list 找一讲 fresh：
```powershell
cargo run --release -- canvas-video list --course-id 88168 --identity wuyutanhongyuxin
```

- [ ] **Step 2：跑单讲 V5.E-B 路径（启用 H2）**
```powershell
$env:RUST_LOG = "info,reqwest=debug,h2=info"
$env:SJTU_NO_FALLBACK = "1"  # 验 H2 真工作不被 fallback 掩盖
mkdir tmp\v5e_smoke -ea 0
cargo run --release -- canvas-video download --video-id <ID> --channel 1 --audio-only `
    --concurrency 8 --to .\tmp\v5e_smoke --identity wuyutanhongyuxin `
    2> tmp\v5e_smoke\L1_h2.stderr.log
```

- [ ] **Step 3：验 H2 协商证据**
```bash
grep -E "h2|HTTP/2|ALPN" tmp/v5e_smoke/L1_h2.stderr.log | head -20
```
Expected: 含 "h2"/"HTTP/2.0" trace。

- [ ] **Step 4：验 elapsed < 2 min**
查 envelope JSON 的 `elapsed_ms`。Expected: < 120,000。

- [ ] **Step 5：失败处置**
- 若 elapsed > 5 min：set `SJTU_FORCE_HTTP1=1` 重跑一次确认 baseline 恢复 V5.D 6.5 min 行为
- 若 H2 work 但 elapsed 在 2-5 min：记 sub-optimal，T4 sweep 后再评
- 若 elapsed < 2 min：✅ 进 T4

- [ ] **Step 6：unset env**
```powershell
Remove-Item env:RUST_LOG
Remove-Item env:SJTU_NO_FALLBACK
```

---

## Task 4: gap_threshold sweep 真机（主对话亲跑，40 min）

**Files:**
- Output: `tmp/v5e_sweep/sweep_results.md`

- [ ] **Step 1：选 4 个不同讲（避免同 video CDN cache 命中污染数据）**

从 list 选 L2/L3/L4/L5。

- [ ] **Step 2：sweep 跑（gap KB ∈ {8, 16, 24, 32}）**
```powershell
foreach ($kb in 8, 16, 24, 32) {
    $env:SJTU_GAP_THRESHOLD_KB = "$kb"
    $env:RUST_LOG = "info"
    $env:SJTU_NO_FALLBACK = "1"
    cargo run --release -- canvas-video download --video-id <L_kb_ID> --channel 1 --audio-only `
        --concurrency 8 --to .\tmp\v5e_sweep --identity wuyutanhongyuxin `
        2> "tmp\v5e_sweep\gap${kb}.stderr.log"
}
Remove-Item env:SJTU_GAP_THRESHOLD_KB
```

- [ ] **Step 3：从 envelope JSON 提关键指标**

| gap KB | range_count | elapsed_ms | bytes_downloaded | written |
|---|---|---|---|---|
| 8 | ? | ? | ? | ? |
| 16 | ? | ? | ? | ? |
| 24 | ? | ? | ? | ? |
| 32 | ? | ? | ? | ? |
| 64 (V5.D baseline) | 1201 | 392850 | 705 MB | 22 MB |

- [ ] **Step 4：写 `tmp/v5e_sweep/sweep_results.md`** 含上表 + 选最优 + 推理。

- [ ] **Step 5：选最优 gap KB**（min elapsed + bytes ≤ V5.D 705 MB），记入 task #47 description。

---

## Task 5: sweep 最优值落硬编码 RANGE_GAP_THRESHOLD_DEFAULT（subagent，15 min）

**Files:**
- Modify: `src/apps/canvas_video/audio_dl/orchestrator.rs`

- [ ] **Step 1：读 T4 sweep 最优值（假设为 N KB）**

- [ ] **Step 2：修改 orchestrator.rs**
```rust
pub(super) const RANGE_GAP_THRESHOLD_DEFAULT: u64 = N * 1024;
```
更新 doc comment 加 V5.E-B sweep 结果引用。

- [ ] **Step 3：跑测确保 effective_gap_threshold_default 测试需要更新（断言改 N * 1024）**

- [ ] **Step 4：cargo test + fmt + clippy**

- [ ] **Step 5：commit**
```bash
git commit -m "feat(canvas-video): V5.E-B-T5 RANGE_GAP_THRESHOLD_DEFAULT = ${N}KB（sweep 最优）"
```

**特殊情况**：若 T4 sweep 表明 64 KB 仍最优 → 仅改 doc comment 加 sweep 数据，const 不动。

---

## Task 6: 9 讲完整 batch 真机（主对话亲跑，~30 min wall time）

**Files:**
- Output: `tmp/v5e_phase2/_comparison.md`, `tmp/v5e_phase2/*.m4a`, `tmp/v5e_phase2/_batch.stderr.log`

- [ ] **Step 1：跑 batch**
```powershell
$env:RUST_LOG = "info"
mkdir tmp\v5e_phase2 -ea 0
cargo run --release -- canvas-video download --batch --course-id 88168 `
    --audio-only --concurrency 8 --to .\tmp\v5e_phase2 `
    --identity wuyutanhongyuxin `
    > tmp\v5e_phase2\_batch.envelope.json `
    2> tmp\v5e_phase2\_batch.stderr.log
```

- [ ] **Step 2：等待完成（预期 < 30 min）**

- [ ] **Step 3：分析 envelope JSON**
- total elapsed
- 每讲 elapsed_ms / bytes_downloaded / download_kind
- fallback 次数（download_kind != m4a-direct）

- [ ] **Step 4：写 `tmp/v5e_phase2/_comparison.md`**

含三方对比表：
| 指标 | V5.B baseline 9 讲 | V5.D L10 估算 × 9 | V5.E-B 实测 9 讲 | V5.E-B vs V5.B |
|---|---|---|---|---|
| total elapsed | 186 min | 59 min | ? | ?× |
| total network | 7.6 GB | 6.3 GB | ? | ?% |
| m4a-direct 比例 | 0/9 | 1/1 | ?/9 | -- |

- [ ] **Step 5：失败处置**
- < 22 min total + 9/9 m4a-direct → ✅ 完美
- 22-40 min total + 8/9 m4a-direct → ✅ 仍合格（部分讲 fallback）
- > 40 min → 调查（看 stderr 是否 retry overhead 或 H2 异常），可能需 V5.E-C

---

## Task 7: 写 lessons + 更新 CLAUDE.md + commit + 关 task #42（subagent，20 min）

**Files:**
- Modify: `tasks/lessons.md`（追加新条目）
- Modify: `CLAUDE.md` (当前阶段段落)
- Modify: `tasks/todo.md`（标 V5.E-B 完成）
- TaskUpdate: #42 → completed

- [ ] **Step 1：写 `tasks/lessons.md` 新条目**

包含：
1. V5.E 设计初值"chunk-level Range"被 ISO 14496-12 §8.7 + 真机 stsc 表证伪 — 教训：spec 期望值要早做物理可行性 sanity check，不只靠 doc 描述推
2. multipart byterange CDN 403 — 教训：CDN WAF 行为黑盒，新 HTTP 高阶 feature 必须先小成本 probe
3. Cargo.toml http2 feature 隐藏 → http1_only redundant — 教训：依赖 feature flag 状态要在 lessons 加 cross-check ritual
4. H2 ALPN + sweep 实测胜果（< 2 min/讲，9 讲 < 30 min）

每条条目 **Why** + **How to apply** 严格落 CLAUDE.md feedback memory 格式。

- [ ] **Step 2：更新 `CLAUDE.md` 当前阶段段落**

```diff
-### 当前阶段
-- **已完成**：S0 / S1 / S2 / S1+S2 瑕疵补丁
-- **下一步**：S3 — 教务（MVP 核心：课表 / 成绩 / GPA）
+### 当前阶段
+- **已完成**：S0 / S1 / S2 / S3 教务 / S3e 电费 / S3 canvas_video V5.B-V5.E-B
+- **下一步**：根据 tasks/todo.md
```
（按真实当前完成度调整）

- [ ] **Step 3：cargo test + fmt + clippy（最终绿）**

- [ ] **Step 4：commit**
```bash
git add tasks/lessons.md CLAUDE.md tasks/todo.md docs/superpowers/
git commit -m "docs(v5e-b): 关 task #42 — H2 multiplex + gap sweep 实战收尾"
```

- [ ] **Step 5：TaskUpdate #42 → completed**
```
TaskUpdate({taskId: "42", status: "completed"})
```

---

## Self-Review Checklist

- [x] **Spec coverage**：每个 spec "改动 1-6" 都在 T1-T7 落到位
- [x] **Placeholder scan**：T3/T4/T6 真机 step 有具体 PowerShell 命令，T1/T2/T5/T7 mechanical step 有完整代码块
- [x] **Type consistency**：`effective_gap_threshold()` / `RANGE_GAP_THRESHOLD_DEFAULT` 名字 T2/T5 一致
- [x] **No 200 行超限**：client.rs / orchestrator.rs 改后均 < 100 行
- [x] **Subagent / 主对话 分工** 清晰：每个 task 头标了
