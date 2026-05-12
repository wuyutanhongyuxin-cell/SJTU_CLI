# V5.E-B+ 实装计划：4-Client H2 Pool + Dynamic P85

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development。每 task 严格 TDD（先写 failing test → 跑确认 fail → 实装 → 跑确认 pass → fmt + clippy → commit）。
> Spec: `docs/superpowers/specs/2026-05-11-v5e-b-plus-multi-h2-p85-design.md`
> Research: `docs/superpowers/research/2026-05-11-v5e-b-cross-validation.md`

**Goal:** V5.D 单讲 6.5 min / 705 MB → V5.E-B+ ~3 min / ~300 MB；9 讲 batch < 30 min。
**Architecture:** 4 独立 reqwest::Client × 每连 100 H2 streams + 1201 range 哈希分桶（range_idx % 4） + Dynamic P85 gap_threshold（解 stco/stsz O(N log N) 算 percentile）。
**Tech Stack:** reqwest 0.12 http2 ALPN，tokio + JoinSet，scraper 无关，新增 0 依赖。

---

## Task 编号约定

- **T1-T4 subagent 跑**（mechanical TDD，sonnet 即可，每 task 含单元测）
- **T0 / T5-T7 main session 亲跑**（真机 CDN，需 SJTU session）
- **T8 main session 收尾**（lessons + commit + 关 task #42）

---

### Task T0: 真机基线快照（main session）

**目的：** 跑 V5.D 当前主线代码单讲一次，拿 elapsed / bytes / range_count / P85 推算（人工解 stco 输出）作为对照基线。无代码改动。

**Files:** 无（只读 + 写 tmp/v5e_baseline/）

- [ ] 跑 `cargo run --release -- canvas video download <某讲 URL> --audio-only` 并记录 elapsed + downloaded
- [ ] log 抓 `range_count` + `total_sample_bytes` + `moov_size`
- [ ] 输出 `tmp/v5e_baseline/<lecture>_v5d_baseline.txt`：elapsed / bytes / range_count / 备注
- [ ] 记录 stderr 是否含 `HTTP/1.1` trace（用 RUST_LOG=reqwest=trace）

**通过条件：** baseline 数据落盘，确认 V5.D 行为符合预期（不该出现 m4a-direct 失败或 fallback）。

---

### Task T1: client.rs → 4-Client H2 池 + 兜底 env（subagent，sonnet）

**Files:**
- Modify: `src/apps/canvas_video/audio_dl/client.rs`
- Test: 同文件 `#[cfg(test)] mod tests`

- [ ] **Step 1: 写 3 个 failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_default_size_is_4() {
        std::env::remove_var("SJTU_FORCE_HTTP1");
        std::env::remove_var("SJTU_H2_POOL_SIZE");
        let pool = build_client_pool_audio("https://courses.sjtu.edu.cn").unwrap();
        assert_eq!(pool.len(), 4);
    }

    #[test]
    fn pool_respects_size_env() {
        std::env::remove_var("SJTU_FORCE_HTTP1");
        std::env::set_var("SJTU_H2_POOL_SIZE", "8");
        let pool = build_client_pool_audio("https://courses.sjtu.edu.cn").unwrap();
        assert_eq!(pool.len(), 8);
        std::env::remove_var("SJTU_H2_POOL_SIZE");
    }

    #[test]
    fn force_http1_falls_back_to_single() {
        std::env::set_var("SJTU_FORCE_HTTP1", "1");
        let pool = build_client_pool_audio("https://courses.sjtu.edu.cn").unwrap();
        assert_eq!(pool.len(), 1);
        std::env::remove_var("SJTU_FORCE_HTTP1");
    }
}
```

注意：env 测试有顺序依赖，必要时加 `#[serial]`（如已用）或保证 setup/cleanup 完整。

- [ ] **Step 2: 跑 `cargo test --lib audio_dl::client` 确认 3 个 fail**

- [ ] **Step 3: 实装 build_client_pool_audio**

撤 `build_client_audio`（保留 build_client_pool_audio），全文替换调用方在 T4 处理。新签名：

```rust
pub(super) fn build_client_pool_audio(referer: &str) -> Result<Vec<Client>> {
    if !referer.is_ascii() {
        return Err(SjtuCliError::InvalidInput(format!("Referer 含非 ASCII 字符：{referer}")).into());
    }
    let mut h = HeaderMap::new();
    h.insert(REFERER, HeaderValue::from_str(referer)
        .map_err(|e| SjtuCliError::InvalidInput(format!("Referer 无效: {e}")))?);
    h.insert(USER_AGENT, HeaderValue::from_static(UA));

    let force_h1 = std::env::var("SJTU_FORCE_HTTP1").as_deref() == Ok("1");
    let pool_size: usize = if force_h1 {
        1
    } else {
        std::env::var("SJTU_H2_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse::<u8>().ok())
            .filter(|&n| (1..=16).contains(&n))
            .map(|n| n as usize)
            .unwrap_or(4)
    };

    let mut pool = Vec::with_capacity(pool_size);
    for _ in 0..pool_size {
        let mut b = Client::builder()
            .default_headers(h.clone())
            .tcp_nodelay(true)
            .tcp_keepalive(Duration::from_secs(60))
            .timeout(Duration::from_secs(90))
            .connect_timeout(Duration::from_secs(15));
        if force_h1 {
            b = b.http1_only().pool_max_idle_per_host(0);
        }
        pool.push(b.build()
            .map_err(|e| SjtuCliError::NetworkError(format!("audio_dl client: {e}")))?);
    }
    Ok(pool)
}
```

更新模块 doc comment：从"关 H2 + 关池"→"4-Client H2 池 + ALPN h2，SJTU_FORCE_HTTP1=1 兜底回 V5.D 行为"。

- [ ] **Step 4: 跑 `cargo test --lib audio_dl::client` 确认 3 测 pass + 其他相邻测无破**

- [ ] **Step 5: 行数 + clippy + fmt + commit**

```bash
wc -l src/apps/canvas_video/audio_dl/client.rs   # 应 < 200
cargo fmt
cargo clippy --lib -- -D warnings
git add src/apps/canvas_video/audio_dl/client.rs
git commit -m "feat(v5e-b+): audio_dl client.rs 单 client → 4-Client H2 池 + SJTU_FORCE_HTTP1/SJTU_H2_POOL_SIZE env"
```

---

### Task T2: ranges.rs → Dynamic P85 gap_threshold（subagent，sonnet）

**Files:**
- Modify: `src/apps/canvas_video/audio_dl/ranges.rs`
- Test: 同文件

- [ ] **Step 1: 写 4 个 failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p85_empty_returns_default() {
        assert_eq!(compute_p85_gap(&[]), 64 * 1024);
    }

    #[test]
    fn p85_single_sample_returns_default() {
        assert_eq!(compute_p85_gap(&[(0, 100)]), 64 * 1024);
    }

    #[test]
    fn p85_bimodal_picks_between_p50_and_p99() {
        // 10 个 small gap (10 KB) + 90 个 large gap (100 KB) — P85 应该落在 100 KB 区段
        // gap = next_off - (cur_off + cur_size)
        let mut samples = vec![(0u64, 1000u32)];
        let mut next = 1000u64;
        for _ in 0..10 { next += 10 * 1024; samples.push((next, 1000)); next += 1000; }
        for _ in 0..90 { next += 100 * 1024; samples.push((next, 1000)); next += 1000; }
        let p85 = compute_p85_gap(&samples);
        assert!(p85 >= 50 * 1024, "p85 应 ≥ 50 KB（落 large gap 区），实得 {p85}");
        assert!(p85 <= 256 * 1024, "p85 应 ≤ clamp max 256 KB，实得 {p85}");
    }

    #[test]
    fn p85_clamps_extremes() {
        // 全 1 KB gap → P85 = 1 KB 但 clamp 到 min 4 KB
        let mut samples = vec![(0u64, 100u32)];
        let mut next = 100u64;
        for _ in 0..50 { next += 1024; samples.push((next, 100)); next += 100; }
        assert!(compute_p85_gap(&samples) >= 4 * 1024);
    }
}
```

- [ ] **Step 2: 跑 `cargo test --lib audio_dl::ranges` 确认 4 个 fail（compute_p85_gap not found）**

- [ ] **Step 3: 实装 compute_p85_gap**

```rust
/// gap_threshold 默认值 / clamp 范围。
const P85_DEFAULT: u64 = 64 * 1024;
const P85_MIN: u64 = 4 * 1024;
const P85_MAX: u64 = 256 * 1024;

/// 计算相邻 audio sample gap 分布的 P85 percentile（作为 merge_ranges 的 gap_threshold）。
///
/// 算法：排序 N-1 个 gap，取 idx = floor(0.85 * (N-1))。
/// 用 P85 而非 P50/P95：audio sample gap 分布 bimodal + 重右尾（V5.D L10 实测），
/// P85 切掉 I-frame 长尾同时保留正常 audio-video 交错合并。
///
/// # Preconditions
/// `samples` 按 offset 升序（与 merge_ranges 一致）。
pub(super) fn compute_p85_gap(samples: &[(u64, u32)]) -> u64 {
    if samples.len() < 2 {
        return P85_DEFAULT;
    }
    let mut gaps: Vec<u64> = Vec::with_capacity(samples.len() - 1);
    for w in samples.windows(2) {
        let cur_end = w[0].0 + w[0].1 as u64;
        let gap = w[1].0.saturating_sub(cur_end);
        gaps.push(gap);
    }
    gaps.sort_unstable();
    let idx = (gaps.len() as f64 * 0.85).floor() as usize;
    let idx = idx.min(gaps.len() - 1);
    gaps[idx].clamp(P85_MIN, P85_MAX)
}
```

- [ ] **Step 4: 跑 `cargo test --lib audio_dl::ranges` 确认 4 测 pass + 既有 merge_ranges 测无破**

- [ ] **Step 5: 行数 + clippy + fmt + commit**

```bash
wc -l src/apps/canvas_video/audio_dl/ranges.rs   # 应 < 200
cargo fmt
cargo clippy --lib -- -D warnings
git add src/apps/canvas_video/audio_dl/ranges.rs
git commit -m "feat(v5e-b+): audio_dl ranges.rs Dynamic P85 gap_threshold（替代固定 64 KB）"
```

---

### Task T3: fetch.rs → parallel_ranges 加 client 池签名（subagent，sonnet）

**Files:**
- Modify: `src/apps/canvas_video/audio_dl/fetch.rs`
- Test: 同文件

- [ ] **Step 1: 写 pick_client 单元测**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;

    #[test]
    fn pick_client_distributes_by_modulo() {
        let pool: Vec<Client> = (0..4).map(|_| Client::new()).collect();
        // 同一 idx 同一 client（指针相等）
        for i in 0..20 {
            let c1 = pick_client(&pool, i);
            let c2 = pick_client(&pool, i);
            assert!(std::ptr::eq(c1, c2));
        }
        // idx 4 应回到 client 0
        assert!(std::ptr::eq(pick_client(&pool, 0), pick_client(&pool, 4)));
        assert!(std::ptr::eq(pick_client(&pool, 1), pick_client(&pool, 5)));
    }
}
```

- [ ] **Step 2: 跑 `cargo test --lib audio_dl::fetch` 确认 fail（pick_client 不存在）**

- [ ] **Step 3: 改 parallel_ranges 签名 + 新增 pick_client**

```rust
/// 在 client 池里按 range_idx 哈希取一个 client（多 H2 连接分桶规避 reqwest #1276）。
fn pick_client(clients: &[Client], range_idx: usize) -> &Client {
    &clients[range_idx % clients.len()]
}

pub(super) async fn parallel_ranges(
    clients: &[Client],
    url: &str,
    ranges: &[(u64, u64)],
    concurrency: usize,
) -> Result<Vec<(usize, Vec<u8>)>> {
    let sem = Arc::new(Semaphore::new(concurrency));
    let mut joins = tokio::task::JoinSet::new();
    for (i, &(s, e)) in ranges.iter().enumerate() {
        let sem = sem.clone();
        let cli = pick_client(clients, i).clone();
        let url = url.to_string();
        joins.spawn(async move {
            let _permit = sem.acquire_owned().await.expect("semaphore closed");
            let bytes = fetch_range_with_retry(&cli, &url, s, e).await?;
            Ok::<_, anyhow::Error>((i, bytes))
        });
    }
    let mut out: Vec<(usize, Vec<u8>)> = Vec::with_capacity(ranges.len());
    while let Some(r) = joins.join_next().await {
        out.push(r.map_err(|e| SjtuCliError::NetworkError(format!("task panic: {e}")))??);
    }
    Ok(out)
}
```

`fetch_range_with_retry` 内部签名 `client: &Client` 不变。

- [ ] **Step 4: 跑 `cargo test --lib audio_dl::fetch` 确认 pass**

- [ ] **Step 5: 行数 + clippy + fmt + commit**

```bash
wc -l src/apps/canvas_video/audio_dl/fetch.rs   # 应 < 200
cargo fmt
cargo clippy --lib -- -D warnings
git add src/apps/canvas_video/audio_dl/fetch.rs
git commit -m "feat(v5e-b+): audio_dl fetch.rs parallel_ranges 接 client 池 + pick_client 哈希分桶"
```

---

### Task T4: orchestrator.rs → 接入 pool + effective_gap_threshold（subagent，sonnet）

**Files:**
- Modify: `src/apps/canvas_video/audio_dl/orchestrator.rs`
- Test: 同文件（env override 单元测）

- [ ] **Step 1: 写 2 个 failing test for effective_gap_threshold**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_env_override_valid() {
        std::env::set_var("SJTU_GAP_THRESHOLD_KB", "16");
        let samples = vec![(0u64, 1u32), (100_000, 1)];  // 任意 ≥2 sample
        assert_eq!(effective_gap_threshold(&samples), 16 * 1024);
        std::env::remove_var("SJTU_GAP_THRESHOLD_KB");
    }

    #[test]
    fn gap_env_invalid_falls_back_to_p85() {
        std::env::set_var("SJTU_GAP_THRESHOLD_KB", "not_a_number");
        // 2 sample 触发 P85 default 64 KB
        let samples = vec![(0u64, 1u32), (100, 1)];
        let g = effective_gap_threshold(&samples);
        assert_eq!(g, 64 * 1024);
        std::env::remove_var("SJTU_GAP_THRESHOLD_KB");
    }
}
```

- [ ] **Step 2: 跑 `cargo test --lib audio_dl::orchestrator` 确认 fail**

- [ ] **Step 3: 改 orchestrator**

撤 `const RANGE_GAP_THRESHOLD`。新增：

```rust
/// 读 SJTU_GAP_THRESHOLD_KB env 强制 override（u32, KB），invalid/unset → compute_p85_gap。
fn effective_gap_threshold(samples: &[(u64, u32)]) -> u64 {
    if let Some(kb) = std::env::var("SJTU_GAP_THRESHOLD_KB")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
    {
        return (kb as u64) * 1024;
    }
    super::ranges::compute_p85_gap(samples)
}
```

`download_audio_only_to_file` 改：

```rust
pub async fn download_audio_only_to_file(...) -> Result<DownloadStats> {
    let pool = super::client::build_client_pool_audio(referer)?;
    let (moov_bytes, mut downloaded) = locate_moov(&pool[0], url).await?;
    // ... parse_moov ...
    let samples: Vec<(u64, u32)> = ...;
    let gap = effective_gap_threshold(&samples);
    info!(gap_threshold_bytes = gap, pool_size = pool.len(),
          "Dynamic P85 + client pool 选定");
    let ranges = super::ranges::merge_ranges(&samples, gap);
    info!(range_count = ranges.len(), sample_count = samples.len(),
          "Range 合并完成");
    let n = concurrency.max(1).min(ranges.len().max(1));
    let fetched = super::fetch::parallel_ranges(&pool, url, &ranges, n).await?;
    // ... 其余不动 ...
}
```

doc comment 同步：撤 const 的位置写 V5.E-B+ 升级背景 + research 文档链接。

- [ ] **Step 4: 跑 `cargo test --lib` 全测 + 确认 audio_dl 测全绿**

注意：依赖 T1 (build_client_pool_audio) + T2 (compute_p85_gap) + T3 (parallel_ranges 新签名) 都完成。

- [ ] **Step 5: 行数 + clippy + fmt + commit**

```bash
wc -l src/apps/canvas_video/audio_dl/orchestrator.rs   # 应 < 200
cargo fmt
cargo clippy --lib -- -D warnings
git add src/apps/canvas_video/audio_dl/orchestrator.rs
git commit -m "feat(v5e-b+): audio_dl orchestrator 接 4-Client 池 + effective_gap_threshold (Dynamic P85 + env override)"
```

---

### Task T5: 真机单讲 H2 smoke（main session）

**目的：** 真实 SJTU CDN 上跑一讲，验证 H2 协商成功 + elapsed < 4 min + log 显示 gap_threshold_bytes 来自 P85（不是 64 KB 默认）。

- [ ] 选取一讲（同 T0 baseline 一致以便对照）
- [ ] 清空 env：`Remove-Item Env:SJTU_FORCE_HTTP1; Remove-Item Env:SJTU_GAP_THRESHOLD_KB; Remove-Item Env:SJTU_H2_POOL_SIZE`
- [ ] 跑 `cargo run --release -- canvas video download <URL> --audio-only`，记 elapsed + bytes
- [ ] log 抓：`gap_threshold_bytes` / `pool_size` / `range_count` / `fetched_bytes`
- [ ] 用 `RUST_LOG=reqwest=debug` 验证 stderr 含 H2 trace（"alpn=h2" or "h2 frame"）
- [ ] 落盘 `tmp/v5e_b_plus/<lecture>_smoke.txt`：elapsed / bytes / pool_size / gap_threshold / vs baseline 比值

**通过条件：**
- ✅ elapsed < 4 min（V5.D baseline 6.5 min × 0.6）
- ✅ stderr H2 trace 出现
- ✅ gap_threshold_bytes ≠ 65536（说明 P85 起作用，不是 default）

**失败处理：** elapsed > 5 min → 跳 T6 SJTU_FORCE_HTTP1 兜底验证；若兜底也 fail，整方案撤回。

---

### Task T6: 真机 P85 vs fixed 64 KB 对照 + SJTU_FORCE_HTTP1 兜底（main session）

**目的：** 数据证明 P85 优于 fixed 64 KB；H1.1 兜底 env 工作。

- [ ] 同一讲：跑 `SJTU_GAP_THRESHOLD_KB=64` env 强制 fixed → 记 elapsed_fixed / bytes_fixed
- [ ] 同一讲：清空 env → P85 → 记 elapsed_p85 / bytes_p85
- [ ] 同一讲：跑 `SJTU_FORCE_HTTP1=1` env → 单 client H1.1 → 记 elapsed_h1 / bytes_h1
- [ ] 落盘 `tmp/v5e_b_plus/<lecture>_p85_vs_fixed.md`：3 行对照表

**通过条件：**
- ✅ bytes_p85 ≤ bytes_fixed（P85 不该比 fixed 64 KB 网络更多）
- ✅ elapsed_h1 与 V5.D baseline 接近（兜底回归）

---

### Task T7: 真机 9 讲完整 batch（main session）

**目的：** 在 9 讲全集上验证 sustained 性能 + 关 task #42。

- [ ] 跑 `cargo run --release -- canvas video batch <course>` 全 9 讲
- [ ] 落盘 `tmp/v5e_b_plus/batch_9lectures.md`：total elapsed + per-lecture elapsed + bytes + download_kind
- [ ] 检查 9/9 `download_kind == "m4a-direct"`

**通过条件（关 #42）：**
- ✅ total < 30 min
- ✅ 9/9 m4a-direct（无 fallback）
- ✅ 9 讲 elapsed σ < 2 min（无某讲炸 10 min+）

⚠ 部分达成（仍关 + 记 lessons）：
- 单讲 3-5 min（次于目标但优于 V5.D）
- 8/9 m4a-direct

❌ 不关：< 8/9 m4a-direct 或 total > 45 min → 标 abandoned

---

### Task T8: lessons + 文档同步 + commit + 关 #42（main session）

**Files:**
- Modify: `tasks/lessons.md`（写 V5.E-B+ 经验）
- Modify: `tasks/todo.md`
- Modify: `CLAUDE.md`（项目结构 audio_dl 模块描述同步：单 client → 4 client pool）

- [ ] 写 lessons：H2 multiplex 实测加速 / Dynamic P85 收益 / reqwest #1276 防御 / SJTU_FORCE_HTTP1 兜底必要性 / CDN H2 行为观察
- [ ] 同步 CLAUDE.md "项目结构" 段：client.rs 描述 + 当前阶段从 "S3 教务" 改回（如果 V5.E-B+ 跑赢就保留 canvas_video MVP 阶段标 done）
- [ ] git add + commit：`feat(v5e-b+): 4-Client H2 + Dynamic P85 收尾 + 9 讲 batch 落地 + lessons`
- [ ] TaskUpdate #42 status=completed

---

## 依赖关系图

```
T0 (baseline)
  ↓
T1 (client.rs)  →  T4 (orchestrator)  →  T5 (smoke)  →  T6 (对照)  →  T7 (batch)  →  T8 (收尾)
T2 (ranges.rs)  →  T4
T3 (fetch.rs)   →  T4
```

T1 / T2 / T3 之间 **完全独立**，但每个 task 都改不同文件 → 顺序派 subagent 避免合并冲突；如果有 worktree 隔离也可并行（本项目主仓直接干，顺序更稳）。

T4 必须等 T1+T2+T3 全完。

T5-T7 必须 main session 亲跑（subagent 没 SJTU session）。

T8 收尾。
