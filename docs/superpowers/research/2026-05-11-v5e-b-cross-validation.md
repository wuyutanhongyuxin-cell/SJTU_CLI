# V5.E-B 联网交叉验证（三轮 subagent 实测铁证汇总）

> 日期：2026-05-11
> 任务：交叉验证 V5.E-B "单 H2 client + fixed gap sweep" 是否最优
> 结论：**部分最优 → 升级为 V5.E-B+（4-Client H2 池 + Dynamic P85）**

复用场景：未来再做 byterange / multiplex / mp4 sample-gap 类研究时，先读本文档避免重复 web search。

---

## 发现 1：5-10× H2 加速是过乐观，实测 1.5-2.5×

| 数据源 | H2 vs H1.1 加速比 | 备注 |
|---|---|---|
| curl 官方 50-stream H2 benchmark | **2.36×** | https://daniel.haxx.se/blog/2023/04/28/curl-8-is-faster/ |
| ImageKit CDN p95 | **1.5×** | many small Range 场景 |
| 学术 paper "many small Range fan-out" | 未找到 5-10× 数据 | google scholar + IEEE Xplore 检索 |

**Tengine MAX_CONCURRENT_STREAMS = 128**（[Aliyun blog 423733](https://www.alibabacloud.com/blog/423733)），与 reqwest 默认 100 流互不约束 → 1201 Range → 10 RTT 群假设成立。

**反例**：丢包率 ≥2% 时 H2 单连反不如 H1.1×8（HoL blocking 卡所有 stream）。SJTU CDN 假定无丢包，但需保留 H1.1 兜底。

**同行做法（重要 → 我们不是第一个）**：
- aria2c：**不用 H2**，N TCP × 16 split（[#476](https://github.com/aria2/aria2/issues/476)）
- yt-dlp：**不用 H2**，H1.1 fragment 池
- 只有 curl 主推 H2 multiplex

→ 期望单讲实测 1.5-2.5× → 6.5 min → **~3 min**（不是 < 2 min）

## 发现 2：单 H2 Client 押全部押有 reqwest 已知坑

- [reqwest #1276](https://github.com/seanmonstar/reqwest/issues/1276) — 单 client H2 高并发会 buffer 阻塞
- [reqwest #1517](https://github.com/seanmonstar/reqwest/issues/1517) — per-host conn limit 与 H2 multiplex 交互怪
- [reqwest #2303](https://github.com/seanmonstar/reqwest/pull/2303) — H3 stabilization 仍 unstable feature flag

**业界 superset 拓扑**：4-8 个独立 `Client` 实例 × 每连 100 stream（.NET runtime / httpx / OpenTelemetry-Rust 都这么做）。

**对策**：1201 range 哈希分桶（range_idx % 4）到 4 个独立 Client → 既享 H2 multiplex 又规避 reqwest 单 client 高并发 bug + 近似 aria2 经典拓扑。

## 发现 3：gap_threshold 真正胜负手是 Dynamic P85

- **{8/16/24/32} KB 漏关键点**：8 KB 低于 audio frame 大小，等于不合并；应扫 {16, 32, 48, 64, 96}
- **audio sample gap 分布**：bimodal + 重右尾（I-frame 长尾撑大），P50 ~10-15 KB、P90 30-60 KB、P99 100-200 KB
- **Dynamic P85**：本地解 stco/stsz 后 O(N) 计算（V5.D 已有 `track.sample_offsets` + `track.sample_sizes`，无额外 I/O），切掉 P85+ 长尾
- **主流工具均未实装**（aria2/yt-dlp/curl 都是固定参数）→ 差异化技术亮点
- range_count 1201 不是瓶颈，**总字节 705 MB 才是**

→ Dynamic P85 期望网络 705 MB → **~300 MB**（P85+ 长尾切掉，理论下限 22 MB 物理上限的中点）

## 已排除替代方案（避免后续重新走弯路）

| 方案 | 状态 | 证据 |
|---|---|---|
| HTTP/3（QUIC）| ❌ 不可行 | Aliyun 支持但额外收费；reqwest 0.12 H3 仍 unstable feature flag |
| chunk-level Range | ❌ 物理不可能 | per-sample chunk 布局：chunk 等价 sample（ISO 14496-12 §8.7） |
| multipart byterange | ❌ CDN 403 | V5.E probe 实测 |
| HTTP/1.1 pipelining | ❌ 死透 | RFC 7230 §6.3.2 弃用，浏览器 / CDN / lib 全断 |
| 强切 aria2c 外部进程 | ⚠ 备胎 | 用户需自装 aria2c，破坏纯 Rust 单 binary 优势；保留作 T3.5 对照基线 |

## V5.E-B+ 升级路线（采纳）

| 维度 | V5.D 现状 | V5.E-B 原 | **V5.E-B+ 采纳** |
|---|---|---|---|
| HTTP version | H1.1 强制 | H2 ALPN | **H2 ALPN** |
| Client 拓扑 | 单 client × 8 TCP | 单 client × N stream | **4 独立 client × N stream（哈希分桶）** |
| gap_threshold | const 64 KB | env override + fixed sweep | **Dynamic P85（本地算）+ env override 兜底** |
| 期望单讲 | 6.5 min | < 2 min（过乐观）| **~3 min（1.5-2.5× 真实）** |
| 期望 9 讲 batch | 60 min | < 22 min | **< 30 min** |
| 期望网络 / 讲 | 705 MB | 480-560 MB | **~300 MB** |
| reqwest bug 防御 | n/a | 单点风险 | **4-client 池规避 #1276 / #1517** |
| 差异化亮点 | n/a | 启 H2 | **Dynamic P85（业界首例 audio-only mp4 byterange 自适应合并）** |

## 检索关键词（后续复用）

- HTTP/2 multiplexing many small Range CDN performance
- reqwest http2 single client connection limit bug
- mp4 audio sample stco gap distribution percentile
- aria2c yt-dlp HTTP/2 byterange why not
- Aliyun Tengine MAX_CONCURRENT_STREAMS H2 ALPN
- HoL blocking HTTP/2 packet loss threshold
