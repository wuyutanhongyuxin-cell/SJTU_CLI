# V5.E-B 设计：启用 HTTP/2 multiplex + gap_threshold real-machine sweep

> 日期：2026-05-11
> 上游 spec：`docs/superpowers/specs/2026-05-10-v5d-audio-only-range-design.md`（V5.D）
> 调研 evidence：本会话 probe_h2 实测 + multipart range 实测 + 两轮 web-cross-validation subagent
> 取代原 V5.E 调研期假设：chunk-level Range（已被 ISO 14496-12 §8.7 + 真机 stsc 表证伪）/ multipart byterange（CDN 403 铁死）

---

## Goal

单讲 6.5 min → **< 2 min** (sustained 跨 9 讲)；网络 705 MB → **≤ 500 MB**；保持 V5.D fail-soft 不破。
最终关掉 task #42。

## 何以胜场（probe 实测铁证）

| 维度 | V5.D 现状 | V5.E-B 改后 | 评估 |
|---|---|---|---|
| HTTP version | HTTP/1.1（`http1_only()` 强制 + Cargo.toml 无 http2 feature 实质性绕过 H2）| **HTTP/2.0**（ALPN 协商，probe A 实测显示 CDN 主动给 H2.0/200/RTT 1021 ms）| ⭐ 核心胜场 |
| 并发结构 | 8 × 独立 TCP（`pool_max_idle_per_host(0)`）| 1 × TCP × 128 H2 streams（reqwest 默认 max_concurrent_streams=100）| RTT 主导场景下 5-10× |
| 1201 Range × 8 batch | 150 RTT 群 | 10 RTT 群 | 墙钟期望 1-2 min |
| gap_threshold | 硬编码 64 KB（gap 浪费 ~683 MB video）| sweep `{8/16/24/32}` KB 取最优落硬编码 | 网络 705 MB → 480-560 MB |
| multipart range | 未试 | 实测 403 Forbidden（CDN WAF 拒绝） | ❌ 死路 |
| chunk-level Range | 未试 | 物理不可能（per-sample chunk 布局：chunk 等价 sample） | ❌ 物理上限 22 MB 不可达 |

**关键数据**：probe_h2 三轮实测（保留 `examples/probe_h2.rs` 作为 V5.E-B 调研资产）
- Probe A（默认 ALPN）：HTTP/2.0 / 200 / 1021 ms / Server: Tengine
- Probe B（http1_only — V5.D 当前）：HTTP/1.1 / 200 / 1010 ms
- Probe C（http2_prior_knowledge）：HTTP/2.0 / 200 / 2414 ms（也工作）

## 改动清单（精确范围）

### 改动 1：`Cargo.toml` reqwest 加 `"http2"` feature ✅（已就位）
```toml
reqwest = { version = "0.12", default-features = false, features = ["cookies", "json", "rustls-tls", "gzip", "http2"] }
```
**Why**：`default-features = false` 关掉了 http2 feature → `http2_prior_knowledge()` / ALPN h2 协商都不可用。这是 V3.1 历史包袱。

### 改动 2：`src/apps/canvas_video/audio_dl/client.rs` 撤强制 H1 + 加 H2 友好配置
```diff
 Client::builder()
     .default_headers(h)
-    .http1_only()
-    .pool_max_idle_per_host(0)
     .tcp_nodelay(true)
+    .tcp_keepalive(Duration::from_secs(60))
     .timeout(Duration::from_secs(90))
     .connect_timeout(Duration::from_secs(15))
     .build()
```
- 撤 `http1_only()`：让 ALPN 自动协商 H2（probe 已证 CDN 支持）
- 撤 `pool_max_idle_per_host(0)`：H2 多路复用要求单 TCP 长连接复用
- 加 `tcp_keepalive(60s)`：长 idle H2 连接被 NAT/防火墙 silently drop 是常见故障，60s keepalive 提前发现
- **不动** 90s 段级 timeout / 15s connect_timeout（V5.D 调好的安全垫）

### 改动 3：`SJTU_FORCE_HTTP1=1` env 兜底
```rust
let mut builder = Client::builder()
    .default_headers(h)
    .tcp_nodelay(true)
    .tcp_keepalive(Duration::from_secs(60))
    .timeout(Duration::from_secs(90))
    .connect_timeout(Duration::from_secs(15));
if std::env::var("SJTU_FORCE_HTTP1").as_deref() == Ok("1") {
    builder = builder.http1_only().pool_max_idle_per_host(0);
}
builder.build()
```
**Why**：H2 真机异常（RST_STREAM 频发 / GOAWAY / 单连接被限速更狠）时一键回到 V5.D 行为。调研 env，不进 `--help`。

### 改动 4：`src/apps/canvas_video/audio_dl/orchestrator.rs` gap_threshold env override
```rust
const RANGE_GAP_THRESHOLD_DEFAULT: u64 = 64 * 1024;

/// 读 SJTU_GAP_THRESHOLD_KB env (u32, KB → bytes)，invalid/unset → 64 KB default
fn effective_gap_threshold() -> u64 {
    std::env::var("SJTU_GAP_THRESHOLD_KB")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .map(|kb| (kb as u64) * 1024)
        .unwrap_or(RANGE_GAP_THRESHOLD_DEFAULT)
}
```
- T4 sweep 后用最优值替换 `RANGE_GAP_THRESHOLD_DEFAULT`
- env override 保留方便后续调研

### 改动 5：测试 client（`test_helpers.rs`）**不动**
**Why**：mockito 是 HTTP/1.1 server，撤 http1_only 会让 reqwest 尝试 H2 ALPN 失败 fallback，徒增不确定性。测试 client 仅供单元测，与生产行为差异已在文档说明。

### 改动 6：相关 doc comment 更新
- `client.rs` module doc：从"关 H2 + 关池"改为"启用 H2 ALPN + 1 TCP × N streams"
- `orchestrator.rs` `RANGE_GAP_THRESHOLD` doc：保留 V5.D 真机数据 + 加 V5.E-B sweep 最优值依据

## 不在范围内（防 scope creep）

- ❌ chunk-level Range：物理不可能（per-sample chunk = chunk 等价 sample）
- ❌ multipart byterange：CDN 403（probe 实测）
- ❌ adaptive concurrency：V5.D fetch_range_with_retry 已 4 次 retry 兜底，足够
- ❌ 其他子系统（jwc / canvas / shuiyuan）的 http1_only：与 V5.E-B 无关，不动
- ❌ download.rs 旧 mp4-full 路径：fallback 必须不动

## Fail-soft 行为不变

- audio_dl 失败仍走 `download_shared.rs` 的 `match Ok / Err → fallback to mp4-full + ffmpeg`
- 加 `SJTU_NO_FALLBACK=1` env 调研期 bypass（V5.D 已实装，不动）

## 验证矩阵

| 测试 | 工具 | 通过条件 |
|---|---|---|
| 单元测：env_override 默认 64 KB | mockito + std::env | ✅ |
| 单元测：env_override `SJTU_GAP_THRESHOLD_KB=8` → 8192 | mockito + std::env | ✅ |
| 单元测：env_override `SJTU_GAP_THRESHOLD_KB=invalid` → 64 KB fallback | mockito + std::env | ✅ |
| 单元测：现有 114 测全绿（撤 http1_only 不破已有） | cargo test | ✅ |
| 真机：单讲 H2 smoke | RUST_LOG=info + bash time | elapsed < 2 min, stderr 含 h2 trace |
| 真机：gap sweep 4 个值 | SJTU_GAP_THRESHOLD_KB env | 选 elapsed 最低 + bytes ≤ V5.D 705 MB |
| 真机：9 讲 batch | cargo run --release --batch | total < 20 min, 9/9 download_kind=m4a-direct |
| 退路：SJTU_FORCE_HTTP1=1 → V5.D baseline 行为 | RUST_LOG=info | HTTP/1.1 + elapsed ~6.5 min |

## 风险 & 缓解

1. **H2 RST_STREAM 高频**：CDN 可能对 1 TCP × 128 stream 不友好（实战阿里云 ENS 行为未知）
   - 缓解：SJTU_FORCE_HTTP1=1 一键回退 V5.D
   - 探测：T3 smoke 失败时立即 fallback test 确认

2. **gap_threshold 不是 monotone**：可能 sweep 4 点都不优于 64 KB
   - 缓解：T5 落硬编码时若 sweep 最优 = 64 KB 则保留不动，仅启用 H2

3. **9 讲 batch CDN 限流**：可能跑到第 5-6 讲触发限流变慢
   - 缓解：fail-soft 已实战验证（V5.D Phase 2 实测），单讲失败不阻断 batch

## 完成判定

✅ task #42 关闭条件：
- T1-T7 全部 ✅
- 9 讲 batch sustained < 22 min total（每讲 < 2.5 min × 9 ≈ 22 min；留 10% buffer）
- 9/9 download_kind=m4a-direct（无 fallback）
- 网络 total ≤ V5.D 估算 6.3 GB × 0.8 = 5 GB
- 单元测全绿（114 → 117+ 新加 3 个 env override 测）

⚠ 部分达成条件（task 仍关闭，但记 V5.E-C 后续）：
- 单讲 2-3 min（H2 不是 10× 是 3-4×）→ 仍关，记 lessons
- 8/9 m4a-direct，1/9 fallback → 仍关，记 lessons
- 网络节省 < 10% → 仍关，gap_threshold 收益不显著

❌ 不关闭条件：
- T3 smoke 单讲 > 5 min（说明 H2 不利或更差）→ 撤 V5.E-B 改动，保留 probe_h2.rs + lessons，task #42 标 abandoned
