# V5 系列 Canvas Video 性能优化复盘（V5.A → V5.F）

> 日期：2026-05-12
> 主题：SJTU Canvas Video 下载 audio-only 加速 —— 3 轮试错 / 1 轮收尾 / 撤回决策
> 复用场景：未来 Claude 做 CDN / Range / mp4 / HTTP 多路复用类性能优化时，**先读本文档**避免重复 web search + 重复试错
> 同档资料：
> - `2026-05-11-v5e-b-cross-validation.md` —— V5.E-B+ 设计前 4 agent web 验证
> - `2026-05-10-v5d-audio-only-range-design.md` ~ `2026-05-12-v5f-mp4-full-plus-ffmpeg-design.md` —— 4 份 spec 系列
> - `tasks/lessons.md` 2026-05-12 段 —— V5.F 5 条规则
> - commits `c0935e3..f3a1721`（V5.D 始 → V5.F 收尾）

---

## 一句话

3 轮 audio-only 优化（V5.B chunk-Range / V5.D sample-Range merge / V5.E-B+ 4-Client H2 池 + Dynamic P85）全部撞墙，最终 V5.F 撤回到 V5.A baseline（mp4-full + ffmpeg）—— 9 讲 batch 15.13 min ≤ 25 min 目标，**接受 916 MB/讲 网络代价换 2 min/讲 稳定 wall-clock**，删 audio_dl/m4a_mux/mp4_box 3 目录共 -2092 行。

---

## 真机对照表（核心数据）

| 阶段 | 路径 | 单讲 elapsed | 9 讲 batch | 单讲网络 | 落盘策略 | 真机结果 |
|---|---|---|---|---|---|---|
| **V5.A baseline** | mp4-full + ffmpeg | ~2 min | ~18 min | 916 MB | 下完抽 m4a 删 mp4 | ✅ |
| **V5.B Phase 1** | mp4 chunk-level Range（H1.1 × 8）| sustained 20.7 min（9th 讲 13 min body 挂死）| —（中断）| 916 MB | 同上 | ❌ body 流挂死 |
| **V5.D** | mp4 box parse + audio sample-level Range merge（gap=64KB）| 6.5 min | ~60 min | 705 MB | 直写 m4a | ⚠ 偏离 < 3 min 目标，705 MB 浪费 32× |
| **V5.E-B+** | 4-Client H2 池 + Dynamic P85 gap | **30.5 min** | > 4 h | 575 MB | 直写 m4a | ❌ **反向退化 4.7×** |
| **V5.F**（撤回） | V5.A baseline 复用 | **1.74 min**（L10 smoke）| **15.13 min** | 916 MB | 下完抽 m4a 删 mp4 | ✅ ≤ 25 min 余量 40% |

各 batch 真机 elapsed 分布（V5.F 9 讲）：1.49 / 1.71 / 2.25 / 1.53 / 1.73 / 1.59 / 1.59 / 1.67 / 1.54 min（平均 1.68 min/讲），全部 ≤ 2.5 min 阈值。

---

## 三轮试错复盘（按时序 + 失败根因）

### V5.B chunk-level Range（mp4 H1.1 × 8）
**假设**：mp4 是 audio chunk + video chunk 交错布局，按 stco 表 chunk 整体 Range 拉，可拿到 ~22 MB 精确 audio。

**真机现实**：SJTU CDN mp4 是 **per-sample chunk** 布局（ISO 14496-12 §8.7 允许 stco entry 每条只装 1 sample），audio chunk = audio sample，audio sample 之间在 mdat 内被 video sample 物理分隔。chunk-Range 实际下了 705 MB（含大量 video noise）。9 讲第 9 讲 body 流挂死 13 min（无 read_timeout 段级保护）。

**死法**：路线根本不通 + 缺 read_timeout 段级保护。

### V5.D sample-level Range merge（gap=64KB）
**假设**：sample-level Range（55699 个 audio sample 各一条 Range）能精确切出 22 MB；用 gap_threshold=64KB 合并相邻 sample，减少 HTTP overhead 到 1201 Range。

**真机现实**：
- 1201 Range 仍然下了 705 MB（与 V5.B 同），因为 audio sample 之间被 video frame 分隔，64KB 合并不足以剔除 video
- 引入 90s 段级 read_timeout + 30s inter-byte timeout 解决 V5.B body 流挂问题
- 改进 `locate_moov`：head 探测时追 mdat box header size 推算下一个 box 起点（绝不假设 offset 是 box 头）
- 引入 `SJTU_NO_FALLBACK=1` env 调试开关 + `download_kind` envelope 字段记录路径
- L10 single-lecture smoke 跑通 6.5 min（vs V5.A 2 min），但 9 讲 batch 未跑（延 V5.E）

**死法**：gap=64KB 不够把 video 切出；要做 chunk-level 切除 video，但 per-sample chunk 布局让 chunk-level 等价 sample-level。**物理不可能**。

### V5.E-B+ 4-Client H2 池 + Dynamic P85 gap
**假设（4 agent 联网验证后）**：
1. H2 multiplexing 对 1201 Range 多路复用，curl 8 benchmark 实测 2.36×，预期单讲 6.5 → ~3 min
2. 4 个独立 Client 池（哈希分桶 range_idx % 4）规避 reqwest #1276 单 client H2 buffer bug
3. Dynamic P85 gap_threshold（本地 O(N log N) 算 stco/stsz 的 sample gap 分布 85 百分位）切掉长尾，705 → ~300 MB

**真机现实**：
- 单讲 elapsed 30.5 min（vs V5.D 6.5 min，**反向退化 4.7×**）
- 9 讲 batch > 4h 不可接受

**死法根因（4 agent 反向 cross-validation 后定位）**：
1. **SJTU CDN 单 H1.1 sustain throughput cap 11.4 MB/s**（fat-pipe 限速，不是 RTT 瓶颈）
2. H2 multiplexing 在 throughput-bound 单大文件场景**等价或更糟**：multiplexing 把单连接带宽切给多个 stream 反而降低有效吞吐
3. Tengine `max_concurrent_streams=128` + `SETTINGS_INITIAL_WINDOW_SIZE=65535` 默认值，1201 range 都挤进单 H2 连接后 flow control window 反复阻塞 + per-stream 窗口频繁同步
4. fail-soft `match Ok/Err → warn + fallback to mp4-full` 让 envelope status=ok 但实际每讲都从 H2 失败降级回 V5.A，30.5 min/讲 完全是 V5.A + ffmpeg 加 H2 setup overhead

### V5.F 撤回（current）
**决策**：删 V5.D + V5.E-B+ 全部 audio-only 路径（17 文件 1500 行），单一路径 V5.A mp4-full + ffmpeg。撤 Cargo.toml `http2` feature + `http1_only()` 强制 H1.1。

**真机现实**：L10 smoke 1.74 min ✓ / 9 讲 batch 15.13 min ✓ / 9/9 mp4-full / 0 failed / 7.86 GB 总下载 / 全部 mp4_kept=false + audio 落盘。

**关上 task #42**（V5.E-B+ 完整 batch 验收），同时**关上 audio-only 优化主线**。

---

## 知识沉淀（搜索过的关键信息 / 复用清单）

### HTTP/2 / HTTP/3 / HTTP/1.1 选择

| 协议 | SJTU CDN 支持 | 适用场景 | 不适用场景 |
|---|---|---|---|
| **HTTP/1.1**（默认）| ✅ | 单大文件 throughput-bound / fat-pipe 限速 / 简单可控 | 多个小请求 RTT-bound |
| **HTTP/2 multiplexing** | ✅（ALPN 协商）| 多个小请求 + 高 RTT + 低丢包 | **单大文件 throughput-bound**（V5.E-B+ 教训）/ 丢包 ≥ 2%（HoL blocking） |
| **HTTP/3 (QUIC)** | ❌ 收费 | 移动 / 高丢包 / 跨国 | **reqwest 0.12 仍 unstable feature** |
| **HTTP/1.1 pipelining** | n/a | n/a | **RFC 7230 §6.3.2 弃用**（浏览器 / CDN / lib 全断）|

**关键加速比真实数据**：
- curl 8 官方 50-stream H2 benchmark：**2.36×**（[daniel.haxx.se 2023-04-28](https://daniel.haxx.se/blog/2023/04/28/curl-8-is-faster/)）
- ImageKit CDN p95 many small Range：**1.5×**
- 学术 paper "many small Range fan-out"：无 5-10× 数据（google scholar + IEEE Xplore 检索）
- **5-10× 是过乐观假设，实测 1.5-2.5× 是 ceiling**

**同行做法**：
- **aria2c 不用 H2**：N TCP × 16 split（[aria2/aria2#476](https://github.com/aria2/aria2/issues/476)）
- **yt-dlp 不用 H2**：H1.1 fragment 池
- 只有 curl 主推 H2 multiplex

### reqwest H2 已知 bug（任何 H2 设计前必查）
- [seanmonstar/reqwest#1276](https://github.com/seanmonstar/reqwest/issues/1276) —— 单 client H2 高并发会 buffer 阻塞
- [seanmonstar/reqwest#1517](https://github.com/seanmonstar/reqwest/issues/1517) —— per-host conn limit 与 H2 multiplex 交互怪
- [seanmonstar/reqwest#2303](https://github.com/seanmonstar/reqwest/pull/2303) —— H3 stabilization 仍 unstable feature flag

**业界 superset 拓扑**：4-8 个独立 `Client` 实例 × 每连 100 stream（.NET runtime / httpx / OpenTelemetry-Rust 都这么做）。**但 throughput-bound 场景下这些都不够好，回到 H1.1**。

### CDN 服务端配置（SJTU live.sjtu.edu.cn）
- **Tengine MAX_CONCURRENT_STREAMS = 128**（[Aliyun blog 423733](https://www.alibabacloud.com/blog/423733)），与 reqwest 默认 100 流互不约束
- **单 H1.1 sustain throughput cap ~11.4 MB/s**（实测，未公开文档 / 可能因校园网 QoS）
- **不支持 multipart byterange**（V5.E probe 实测 CDN 返 403）
- **不提供 audio-only endpoint**（11 variant URL 探针全 404，含 `.m4a` / `.aac` / `?format=audio` / `/audio/` 路径变体）
- **HoL blocking 在丢包 ≥ 2% 时 H2 单连反不如 H1.1×8**（卡所有 stream）；SJTU 校园网假定无丢包，但需保留 H1.1 兜底

### mp4 box 结构知识

**hexdump 验证范式（5 min 验真比 5 day 写代码值）**：
```
00000000: 0000 0018 6674 7970 6d70 3432  ← ftyp box (24 bytes)
00000010: ... ftyp 内容 ...
00000018: NNNN NNNN 6d64 6174           ← mdat 紧接 ftyp（size = 0xNNNNNNNN）
        OR
00000018: NNNN NNNN 6d6f 6f76           ← moov 紧接 ftyp（faststart）
```

- **moov-end / non-faststart**：`ftyp[24] → mdat[915MB] → moov[2.1MB]` —— SJTU CDN 是这种
- **faststart**：`ftyp[24] → moov → mdat` —— ffmpeg stdin pipe 流式抽流需要此种
- **决策**：moov-end mp4 stdin pipe 抽流不可行（ffmpeg 无法 seek 回头读 moov）—— **B.1 微优化否决**

**关键 box 解析陷阱**：
- mp4 是 box 流，box 之间无 magic separator，**绝不能从中间字节解析**（V5.D 早期 bug：尾部翻倍探测从 `total - probe` 倒退，落在 mdat 内部把随机字节当 box header）
- 必须从合法 box 边界开始 scan：head 探测时追 ftyp / mdat box header 的 size 字段，推算下一个 box 起点

**ISO 14496-12 §8.7 chunk-vs-sample**：
- mp4 允许 stco entry 每条只装 1 sample（**per-sample chunk** 布局）
- 这意味着 chunk-level Range = sample-level Range，"chunk 整体下"对 per-sample chunk 布局**物理不可能**剔除 video
- audio sample 在 mdat 内被 video sample 分隔，gap=0 → 5w+ Range HTTP overhead 爆炸；gap=64KB → 1201 Range 但单段含大量 video

### Range 优化的实际经济性

| 策略 | Range 数 | 网络字节 | HTTP overhead | 总耗时（V5.D 实测）|
|---|---|---|---|---|
| 全下 mp4 | 1（或 ~50 段并发） | 916 MB | 极低 | **~2 min**（fat-pipe 跑满）|
| sample-level gap=0 | 55699 | ~22 MB（理论）| HTTP 头爆炸 | 远 > 5 min |
| sample-level gap=64KB | 1201 | 705 MB | 适中 | 6.5 min |
| Dynamic P85 gap | ~3000-5000 | ~575 MB | 适中 | 30.5 min（H2 反向退化）|

**核心 insight**：**fat-pipe sustain throughput 才是瓶颈**，Range 数和总字节都是次因。继续追"少下点 byte"在 11.4 MB/s 限速面前没收益（916 MB ÷ 11.4 MB/s ≈ 80s = 1.3 min，加 ffmpeg 抽流 20s = 总 ~1.7 min，与 V5.F 实测一致）。

---

## 已排除方案矩阵（**禁止重新走**）

| 方案 | 状态 | 否决证据 |
|---|---|---|
| mp4 chunk-level Range | ❌ 物理不可能 | per-sample chunk 布局（ISO 14496-12 §8.7） |
| sample-level Range gap merge | ❌ 经济性差 | gap=64KB 仍 705 MB；gap=任何值 video 都切不干净 |
| HTTP/2 multiplexing 单大文件 | ❌ 反向退化 | V5.E-B+ 30.5 min/讲 vs V5.A 2 min/讲 |
| 4-Client H2 池 + 哈希分桶 | ❌ 反向退化 | 同上，throughput-bound 场景拓扑无收益 |
| Dynamic P85 自适应 gap | ❌ 反向退化 | 575 MB 节省被 H2 反向退化吃光 |
| ffmpeg stdin pipe（B.1）| ❌ 格式不支持 | moov-end mp4 ffmpeg 无法 seek（hexdump 5 min 证实）|
| 自实现 qt-faststart 重排 moov | ❌ 等价 V5.A | 需先下完整 mp4 |
| audio-only endpoint | ❌ CDN 不提供 | 11 variant URL probe 全 404 |
| HTTP/3 (QUIC) | ❌ 不可行 | Aliyun 支持但收费 / reqwest 0.12 H3 unstable |
| multipart byterange | ❌ CDN 不支持 | V5.E probe 实测 CDN 403 |
| HTTP/1.1 pipelining | ❌ 协议弃用 | RFC 7230 §6.3.2 / 浏览器 + lib 全断 |
| 强切 aria2c 外部进程 | ⚠ 备胎不采纳 | 破坏纯 Rust 单 binary 优势 |

---

## 复用规则（什么场景再考虑重启）

1. **CDN 改架构升级 ALPN h3 / 提供 audio-only endpoint** → 重启 audio-only 路径，先 hexdump + 11 variant probe 验证
2. **校园网 QoS 提升、单 H1.1 sustain > 50 MB/s** → V5.A 仍是最优解，不需要 audio-only
3. **目标变成"省用户磁盘"而非"省时间"** → 加 `--audio-only --stream-to-stdout` 直管 stdout 模式，下 mp4 同时 ffmpeg pipe 抽 m4a 直 stdout，可避免 916 MB 临时落盘（但仍需下 916 MB 网络）
4. **遇到 throughput-bound vs RTT-bound 不确定** → 用 curl 单连接跑 5 min 测 sustain throughput；< 5 MB/s → RTT-bound 可考虑 H2；> 10 MB/s → 几乎一定 throughput-bound，H1.1 + 8 split conn 即最优
5. **任何"切 Range 省字节"假设** → 先用 hexdump 看格式实际布局，再用 stco/stsz 分布算理论上限，**别相信论文 / AWS S3 文档**

---

## 检索关键词（后续 web search 复用）

- HTTP/2 multiplexing many small Range CDN performance benchmark
- reqwest http2 single client connection limit bug
- mp4 audio sample stco gap distribution percentile chunk
- aria2c yt-dlp HTTP/2 byterange why not
- Aliyun Tengine MAX_CONCURRENT_STREAMS H2 ALPN
- HoL blocking HTTP/2 packet loss threshold
- ISO 14496-12 per-sample chunk stco entry
- moov-end vs faststart mp4 ffmpeg stdin pipe
- HTTP/1.1 pipelining RFC 7230 deprecation
- HTTP/3 QUIC reqwest unstable feature flag
- multipart byterange CDN 403 nginx
- fat-pipe sustain throughput single TCP connection cap

---

## 引用（spec / plan / lessons / commit）

**Specs（按时序）**：
- `docs/superpowers/specs/2026-05-10-v5d-audio-only-range-design.md` —— V5.D 设计（手刻 mp4 parser + 直写 m4a）
- `docs/superpowers/specs/2026-05-11-v5e-b-h2-gap-sweep-design.md` —— V5.E-B 原版（单 client + fixed sweep，被 superseded）
- `docs/superpowers/specs/2026-05-11-v5e-b-plus-multi-h2-p85-design.md` —— V5.E-B+（4-Client H2 池 + Dynamic P85）
- `docs/superpowers/specs/2026-05-12-v5f-mp4-full-plus-ffmpeg-design.md` —— V5.F 撤回

**Plans（按时序）**：
- `docs/superpowers/plans/2026-05-10-v5d-audio-only-range.md`
- `docs/superpowers/plans/2026-05-11-v5e-b-h2-gap-sweep.md`
- `docs/superpowers/plans/2026-05-11-v5e-b-plus-multi-h2-p85.md`
- `docs/superpowers/plans/2026-05-12-v5f-mp4-full-plus-ffmpeg.md`

**Research（4 agent web 验证）**：
- `docs/superpowers/research/2026-05-11-v5e-b-cross-validation.md` —— V5.E-B+ 前 4 agent 验证（升级 V5.E-B → V5.E-B+ 的依据）
- 本文档 —— V5 系列收尾复盘 + 知识沉淀

**Lessons**：
- `tasks/lessons.md` 2026-05-12 段 —— V5.F 5 条规则（CDN 真实约束先验 / H2 throughput-bound vs RTT-bound / 实验路径关 fail-soft / 优化走 sidetrack / 撤回按绝对值）
- `tasks/lessons.md` 2026-05-11 段 —— V5.D mp4 真实布局 + sample-level Range 工程妥协

**关键 commits**：
- `c0935e3..29d05d1` —— V5.E-B+ 实装（4 commits）
- `d2761e2` —— V5.F 撤 download_shared 撤 V5.D audio_dl 调用
- `3b3b9c6` —— V5.F 删 audio_dl/m4a_mux/mp4_box 3 目录 -2092 行
- `f3a1721` —— V5.F 收尾 docs + lessons + retrospective（本文档）

---

## 仓库决策记录

- V5.D / V5.E-B+ commits **不 revert**，保留 git history audit trail；本 retrospective 是其墓志铭
- audio_dl/m4a_mux/mp4_box 三模块代码**不归档分支**（删干净）。未来若重启需重写。归档复活率 < 5%，徒增 noise
- `download_kind` envelope 字段**保留 `mp4-full` / `skipped` 两个取值**，不删（envelope additive 契约，下游 AI Agent 可能依赖）
