# V5.E-B+ 设计：4-Client H2 Pool + Dynamic P85 gap_threshold

> 日期：2026-05-11
> 上游 spec：`2026-05-10-v5d-audio-only-range-design.md`（V5.D 主线）
> Supersedes：`2026-05-11-v5e-b-h2-gap-sweep-design.md`（V5.E-B 原版 — 单 Client + fixed sweep）
> 升级证据：`docs/superpowers/research/2026-05-11-v5e-b-cross-validation.md`（三轮 web 验证）

---

## 一句话

启用 HTTP/2 ALPN multiplex（撤 V5.D 的 `http1_only()`），把单 Client 升级为 **4-Client 池 + range 哈希分桶**（规避 reqwest #1276 单 client H2 高并发 buffer bug），并把 V5.D 硬编码 `RANGE_GAP_THRESHOLD = 64 KB` 替换为 **Dynamic P85 gap_threshold**（本地解 stco/stsz O(N) 算 P85 percentile）。

## Goal

| 指标 | V5.D 现状 | V5.E-B+ 目标 |
|---|---|---|
| 单讲 elapsed | 6.5 min | **~3 min**（1.5-2.5× 真实加速）|
| 9 讲 batch | ~60 min | **< 30 min** |
| 单讲网络字节 | 705 MB | **~300 MB**（P85 cut 切长尾）|
| H2 协商 | ❌ http1_only | ✅ ALPN h2 |
| reqwest H2 bug 防御 | n/a | ✅ 4 Client 池 |
| Fail-soft 不破 | ✅ | ✅（仍走 mp4-full + ffmpeg）|

## 改动清单（精确到文件 / 行）

### 改动 1：`Cargo.toml` reqwest http2 feature ✅（V5.E-B 已就位，不动）
```toml
reqwest = { version = "0.12", default-features = false, features = ["cookies", "json", "rustls-tls", "gzip", "http2"] }
```

### 改动 2：`src/apps/canvas_video/audio_dl/client.rs` 单 Client → 4-Client 池
- 旧：`pub(super) fn build_client_audio(referer: &str) -> Result<Client>`
- 新：`pub(super) fn build_client_pool_audio(referer: &str) -> Result<Vec<Client>>`
- 实装：
  - 默认建 **4** 个独立 `Client`，每个 `tcp_keepalive(60s) + tcp_nodelay + timeout(90s) + connect_timeout(15s)`，**不**调 `http1_only()` / `pool_max_idle_per_host(0)`（让 ALPN 协商 H2）
  - 读 `SJTU_FORCE_HTTP1=1` env → 退到 **单 Client + http1_only + pool_max_idle_per_host(0)**（V5.D 行为，兜底）
  - 读 `SJTU_H2_POOL_SIZE` env（u8, 1-16, 默认 4，invalid 取默认）→ 调试用，不进 `--help`
- doc comment 写明每个 client 独立 H2 连接 → 1201 range 哈希分桶到 4 个 client，规避 reqwest #1276

### 改动 3：`src/apps/canvas_video/audio_dl/fetch.rs` parallel_ranges 加 client 池签名
- 旧签名：`parallel_ranges(client: &Client, url, ranges, concurrency)`
- 新签名：`parallel_ranges(clients: &[Client], url, ranges, concurrency)`
- 实装：新增内部 helper `fn pick_client(clients: &[Client], range_idx: usize) -> &Client { &clients[range_idx % clients.len()] }`，单元可测
- fetch_range_with_retry 签名跟着改成 `&Client`（值不变）
- 调用方 orchestrator 改传 `&clients[..]`

### 改动 4：`src/apps/canvas_video/audio_dl/ranges.rs` 新增 Dynamic P85
```rust
/// 计算相邻 audio sample 的 gap 分布的 P85 percentile，作为 merge_ranges 的 gap_threshold。
///
/// 算法：O(N log N)（排序 N-1 个 gap），N = sample_count（V5.D L10 实测 ~55k）。
/// 用 P85（不是 P50/P95）：P50 太低会留下大量小 Range；P95 切不掉长尾；P85 是 bimodal 分布
/// 的"谷底" — 大多数正常 audio-video 交错 gap < 30 KB，I-frame 长尾 gap > 100 KB 被切。
///
/// # Returns
/// - 0 sample → 64 KB default
/// - 1 sample → 64 KB default（无 gap 可算）
/// - ≥2 sample → P85 gap（最小 4 KB，最大 256 KB，超界 clamp）
pub(super) fn compute_p85_gap(samples: &[(u64, u32)]) -> u64 { ... }
```
- 单元测 4 个：空 / 单个 / bimodal 分布 / clamp 边界

### 改动 5：`src/apps/canvas_video/audio_dl/orchestrator.rs` 替换 const + 用池
- 撤 `const RANGE_GAP_THRESHOLD: u64 = 64 * 1024;`
- 新加 `fn effective_gap_threshold(samples: &[(u64, u32)]) -> u64`：
  1. 优先 `SJTU_GAP_THRESHOLD_KB` env override（V5.E-B 兜底机制保留）
  2. 否则 `compute_p85_gap(samples)`
- `download_audio_only_to_file`：
  - `build_client_pool_audio()` 拿 `Vec<Client>`
  - `locate_moov` / 第一个 fetch_range 用 `&pool[0]`（任意一个，复用 H2 连接）
  - `parallel_ranges(&pool, ...)` 传整个池
  - `let gap = effective_gap_threshold(&samples);`
  - `let ranges = merge_ranges(&samples, gap);`
  - log 打 `gap_threshold_bytes` 让真机调研可观察

### 改动 6：`src/apps/canvas_video/audio_dl/locate.rs` fetch_range 签名跟改
- 改 `&Client` 为 `&Client`（实际本来就是 `&Client`，确认无需动）
- `locate_moov(client: &Client, url)` 仍接 `&Client`，调用方传 `&pool[0]`

### 改动 7：测试 client `test_helpers.rs` **不动**
mockito 是 H1.1 server，撤 http1_only 会让 reqwest 尝试 H2 ALPN 失败 fallback，徒增不确定性。

### 改动 8：相关 doc comment 同步
- `client.rs` module doc：从"关 H2 + 关池"→"4-Client H2 池 + ALPN h2"
- `orchestrator.rs` 撤 const 处 doc：保留 V5.D 真机背景 + 加 Dynamic P85 依据 + research 文档链接
- `fetch.rs` parallel_ranges doc：写明 client 池由 range_idx 哈希分桶

## 测试矩阵

| 测试 | 工具 | 通过条件 |
|---|---|---|
| `ranges::compute_p85_gap` 空 | unit | == 64 KB |
| `ranges::compute_p85_gap` 单 sample | unit | == 64 KB |
| `ranges::compute_p85_gap` bimodal | unit | 落 P50<x<P95 区间 |
| `ranges::compute_p85_gap` clamp 边界 | unit | min 4 KB max 256 KB |
| `client::build_client_pool_audio` 默认 | unit | Vec.len() == 4 |
| `client::build_client_pool_audio` SJTU_FORCE_HTTP1=1 | unit + std::env | Vec.len() == 1 |
| `client::build_client_pool_audio` SJTU_H2_POOL_SIZE=8 | unit + std::env | Vec.len() == 8 |
| `fetch::pick_client` 分桶 | unit | range_idx 0/1/2/3/4 → client 0/1/2/3/0 |
| `orchestrator::effective_gap_threshold` env override | unit + std::env | 16 → 16384 |
| `orchestrator::effective_gap_threshold` env invalid | unit + std::env | fallback P85 |
| 现有 114 单元测 | cargo test | 全绿 |
| 真机 T3：单讲 H2 smoke | bash time + RUST_LOG=info | elapsed < 4 min, log 含 h2 trace + gap_threshold_bytes |
| 真机 T4：Dynamic P85 vs fixed 对照 | SJTU_GAP_THRESHOLD_KB=64 vs 默认 | 默认 elapsed ≤ fixed elapsed，bytes 默认 ≤ fixed |
| 真机 T5：SJTU_FORCE_HTTP1=1 兜底 | env | 走 H1.1 + V5.D baseline 行为 |
| 真机 T6：9 讲 batch | cargo run --release --batch | total < 30 min, 9/9 m4a-direct |

## 不在范围（防 scope creep）

- ❌ chunk-level Range：per-sample chunk 物理上限不可达（V5.E 已证）
- ❌ multipart byterange：CDN 403（probe 证）
- ❌ HTTP/3：reqwest H3 unstable + Aliyun 收费
- ❌ aria2c 外部进程：破坏纯 Rust 单 binary 优势
- ❌ adaptive concurrency：fetch_range_with_retry 4 次 retry 已兜底
- ❌ download.rs 旧 mp4-full 路径不动（fallback 仍走它）

## Fail-soft 行为不变

- audio_dl 失败仍走 `download_shared.rs` 回退 mp4-full + ffmpeg
- `SJTU_NO_FALLBACK=1` env 调研期 bypass（V5.D 已实装，不动）

## 完成判定（关闭 task #42）

✅ 关闭：
- 所有 T0-T8 完成
- 9 讲 batch sustained **< 30 min total**
- 9/9 `download_kind=m4a-direct`（无 fallback）
- 网络 total ≤ V5.D × 0.6 ≈ 3.8 GB
- 单元测全绿（114 → 124+ 含新加）

⚠ 部分达成（仍关 + 记 lessons）：
- 单讲 3-4 min（H2 不是 2× 是 1.5×）
- 8/9 m4a-direct，1/9 fallback
- 网络节省 < 30%

❌ 不关闭：
- 真机 T3 smoke 单讲 > 5 min → V5.E-B+ 全撤回，task #42 abandoned，留 research + lessons

## 风险 & 缓解

1. **H2 RST_STREAM 频发**：4-Client 池放大概率
   - 缓解：`SJTU_FORCE_HTTP1=1` 一键回 V5.D 单 client H1.1
   - 探测：T3 smoke 失败 → 立即 T5 兜底验证

2. **Dynamic P85 算偏**：bimodal 分布不典型时 P85 退化
   - 缓解：`SJTU_GAP_THRESHOLD_KB` env override 强制固定值
   - 探测：T4 对照实测验证 P85 ≤ fixed 64 KB

3. **4 Client 同时握手开销**：connect_timeout × 4 可能堆叠
   - 缓解：tokio::spawn 并行 build_client 不串行；4 × 15s 上限可接受

4. **CDN 限流 4 Client**：Aliyun 可能针对单 IP 4 conn 不友好
   - 缓解：V5.D Phase 2 实测 8 conn 无限流，4 conn 应无问题；fail-soft 兜底
