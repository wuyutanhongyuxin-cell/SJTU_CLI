# V5.D 设计 — Canvas 课堂视频 audio-only Range 加速 + read_timeout 收紧

**状态**：spec 等待用户复核 → writing-plans
**日期**：2026-05-10
**前置**：V5.A/V5.B Phase 1（前 9 讲基线，见 `tmp/v5_audio_18/_baseline.md`）/ V5.C 已完成。working tree dirty（V5.B 中断时残留）。

## 摘要

旧 audio-only 路径下载整个 mp4（~840 MB/讲）→ ffmpeg 抽 ~20 MB m4a → 删 mp4，**网络浪费比 42×**。V5.D 引入 mp4 box parser，**只 Range 拉 audio track 的 sample 字节**，本地 mux 成 m4a，省去 ffmpeg。同时收紧 reqwest 段级 timeout（30 min → 90 s）+ 加 chunk 间 inter-byte timeout（30 s），根治 V5.B Phase 1 第 9 讲 13 min body 流挂死。

预期：单讲网络 840 MB → 22 MB（**~38× 节省**）/ 单讲墙钟 sustained 21 min → 3-5 min（**~5× 提速**）。

## 范围

**In scope**：
- 手写最小化 mp4 box parser（仅解析必要 box）
- 手写 m4a muxer（重建一个 audio-only mp4 容器）
- audio-only 专属下载入口（旧 mp4 全下路径不动）
- audio-only 专属 reqwest Client（90 s 段级 + 30 s inter-byte），不影响旧 mp4 client
- ChannelOutput envelope 字段语义微调（`bytes` 在 audio-only 新路径为 m4a 字节而非 mp4 字节）

**Out of scope（V5.E+ 留）**：
- 跨讲 Semaphore 并发池
- 全路径迁移（普通 mp4 下载继续走 V3.1）
- 字幕 / 转录（V5.C 已调研，V6 主题）

## 关键决策（brainstorming Q1-Q4 已对齐）

| 决策点 | 选择 | 备选 | 理由 |
|---|---|---|---|
| mp4 parser | 手写最小化 | mp4 crate / mp4parse crate | CLAUDE.md 禁自引依赖；只用 ~5% box 类型 |
| 范围 | 仅 audio-only 路径 | 全路径重写 / 同时做并发池 | 旧路径稳定不动，回滚容易；A/B 对照变量单一 |
| read_timeout | 90 s 段级 + 30 s inter-byte | 180 s / 30 min 不动 | 90 s 远宽于 audio-only 单段（~3 MB < 5 s）；inter-byte 抓 body 流挂死 |
| 工作流 | spec → plan → subagent | inline 直接做 / 用户拍板范围 | non-trivial 任务走完整流程，subagent 隔离 context |

## 架构

```
audio-only 下载新流程（V5.D）：

URL ──► [HEAD probe size]──┐
                           ▼
                    [Range 0..1MB 取头部]
                           ▼
                ┌──── 头部含 moov？───┐
                │                    │
              YES                    NO（mdat 在前 = 非 faststart）
                │                    │
                │                    ▼
                │             [Range size-1MB..size 取尾]
                │                    │
                │                    ▼
                │             [循环找 moov box]
                ▼                    │
          ◄──── 拿到 moov 字节 ◄─────┘
                           ▼
              [parse moov → AudioTrack { offsets, sizes }]
                           ▼
              [合并相邻 sample → Range chunks（gap < 64KB 合并）]
                           ▼
              [并发 8 路 Range 取 audio sample bytes]
                           │
                           ▼
              [按原 sample 顺序拼装 mdat payload]
                           ▼
              [构造 m4a：ftyp + moov（重写 stbl 的 stco 指向新 mdat 起点）+ mdat]
                           ▼
                       写盘 m4a
```

**对比旧流程**（保留不动）：
```
URL → 全 Range 下 840 MB mp4 → ffmpeg -vn -acodec copy → 20 MB m4a → 删 mp4
```

## 模块布局

```
src/apps/canvas_video/
├── mp4_box/                 # NEW 拆 3 子文件 < 200 行
│   ├── mod.rs              # ~30 行 — re-export AudioTrack / parse_moov
│   ├── parser.rs           # ~180 行 — ftyp / moov / trak / stbl 解析
│   └── tests.rs            # ~120 行 — 用 ffmpeg 生成的小 mp4 fixture 测
├── m4a_mux/                # NEW 拆 2 子文件 < 200 行
│   ├── mod.rs              # ~150 行 — write_m4a(out, audio_track, sample_bytes)
│   └── tests.rs            # ~100 行 — round-trip + ffprobe 验
├── audio_dl/               # NEW 拆 3 子文件 < 200 行
│   ├── mod.rs              # ~30 行 — pub use download_audio_only_to_file
│   ├── client.rs           # ~80 行 — build_client_audio()（90 s + inter-byte）
│   ├── orchestrator.rs     # ~180 行 — moov 定位 + Range 拼装 + mux
│   └── tests.rs            # ~120 行 — mockito CDN 慢响应 / inter-byte 误伤
├── download.rs              # 不动（200 行已贴顶）
├── ffmpeg.rs                # 不动（仍给 keep-mp4 路径用）
└── mod.rs                   # +3 行（mod mp4_box / m4a_mux / audio_dl）

src/commands/canvas_video/
└── download_shared.rs       # +~10 行，audio_only 分流到 audio_dl，旧路径不变
```

## 关键接口签名

### mp4_box::AudioTrack

```rust
// src/apps/canvas_video/mp4_box/mod.rs
pub struct AudioTrack {
    /// AAC / mp4a 等 codec 名
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u8,
    /// 每个 sample 的绝对偏移（mp4 文件内）
    pub sample_offsets: Vec<u64>,
    /// 每个 sample 的字节数
    pub sample_sizes: Vec<u32>,
    /// 时间戳信息（mvhd timescale + mdhd duration），mux 时回填
    pub mvhd_timescale: u32,
    pub mdhd_timescale: u32,
    pub mdhd_duration: u64,
    /// 原 trak 的 stsd 字节（含 mp4a entry + esds 配置），mux 时直接复用
    pub stsd_raw: Vec<u8>,
}

pub fn parse_moov(moov_bytes: &[u8]) -> Result<AudioTrack>;
```

### audio_dl::download_audio_only_to_file

```rust
// src/apps/canvas_video/audio_dl/mod.rs
/// 下载 url 的 mp4 中 audio track，本地 mux 成 m4a 落到 dest_m4a。
/// 不下载视频字节、不调 ffmpeg。返回写入 m4a 的字节数（远小于 mp4 size）。
pub async fn download_audio_only_to_file(
    url: &str,
    dest_m4a: &Path,
    concurrency: usize,
    referer: &str,
) -> Result<u64>;
```

### m4a_mux::write_m4a

```rust
// src/apps/canvas_video/m4a_mux/mod.rs
/// 把 audio_track + sample_bytes 拼成最小化 m4a（ftyp + moov + mdat）。
/// sample_bytes 必须按 audio_track.sample_offsets 顺序拼好（紧密排列，无 padding）。
pub fn write_m4a(
    out: &Path,
    audio_track: &AudioTrack,
    sample_bytes: &[u8],
) -> Result<u64>;
```

### audio_dl::client::build_client_audio

```rust
// src/apps/canvas_video/audio_dl/client.rs
/// audio-only 专属 reqwest Client：90 s 段级 timeout，关 H2 + 关池（继承 V3.1 经验）。
pub(super) fn build_client_audio(referer: &str) -> Result<Client>;
```

`download.rs` 内的 `build_client` **保持不动**，旧 mp4 路径继续 30 min timeout。

### inter-byte timeout 内联（不抽 helper）

orchestrator.rs 里 chunk 循环：
```rust
const INTER_BYTE_TIMEOUT: Duration = Duration::from_secs(30);
loop {
    let chunk = tokio::time::timeout(INTER_BYTE_TIMEOUT, resp.chunk())
        .await
        .map_err(|_| SjtuCliError::NetworkError("段 chunk 30s 无字节流入".into()))??;
    let Some(c) = chunk else { break };
    // write to buffer
}
```

## moov 定位算法

mp4 文件 layout 两种：
- **faststart**（Web 优化）：`ftyp + moov + mdat` → 头部 1 MB 内含 moov
- **标准**：`ftyp + mdat + moov` → moov 在尾部

策略（orchestrator.rs）：
1. `Range: bytes=0-1048575` 拿前 1 MB
2. 跳过 ftyp box，看下一个 box type：
   - `moov` → 直接读完整 moov（可能跨 1 MB 边界，看 box size 决定是否补 Range）
   - `mdat` 或其他 → moov 在尾部
3. moov 在尾部时：`Range: bytes={size-N}-{size-1}` 取最后 N MB（首发 1 MB，不够再翻倍到 16 MB 上限）
4. 在尾部字节里从后向前找 `moov` box header（4 字节 type + 前面 4 字节 size）

实测 SJTU CDN（v.sjtu.edu.cn）的 mp4 多半是标准 layout（mdat 在前），所以多半要走第 4 步。

## Range 合并策略

audio sample 通常是密集排列（mp4 容器内 audio sample 多挨在一起，但每个 sample 之间可能插了几 KB 视频 sample）。

合并算法：
```rust
// gap 阈值：< 64 KB 的间隙直接 inline 多下，节省请求数（CDN per-conn 开销 > 多下 64 KB）
fn merge_ranges(samples: &[(u64, u32)], gap_threshold: u64) -> Vec<(u64, u64)> {
    // (offset, size) -> Vec<(start_inclusive, end_inclusive)>
    let mut merged = Vec::new();
    let mut cur_start = samples[0].0;
    let mut cur_end = samples[0].0 + samples[0].1 as u64 - 1;
    for &(off, size) in &samples[1..] {
        if off <= cur_end + 1 + gap_threshold {
            cur_end = (off + size as u64 - 1).max(cur_end);
        } else {
            merged.push((cur_start, cur_end));
            cur_start = off;
            cur_end = off + size as u64 - 1;
        }
    }
    merged.push((cur_start, cur_end));
    merged
}
```

预期合并率：~3000 audio samples / 20 min 课 → 合并后 ~50-100 个 Range request（gap 64 KB 时）。

## envelope 兼容性（additive 设计）

**联网交叉验证结论**（Confluent / Conduktor / Creek 三家 schema-evolution 文档共识）：改字段语义 = breaking change，应避免；**加可选字段 + default = 安全的 additive change**。yt-dlp 同领域参考也是用 `filesize` / `filesize_approx` 多字段分语义而不是覆盖单字段。

**V5.D envelope 改动 = 纯 additive，旧字段语义/类型/形状全部保留不动**：

`ChannelOutput`（`src/commands/canvas_video/data.rs:104-115`）：

```rust
pub struct ChannelOutput {
    pub channel: i32,
    pub file_path: String,          // 不动：仍是 mp4 路径占位（V4 quirk）
    pub audio_path: Option<String>, // 不动：m4a 真实路径（V4 / V5.D 都对）
    pub mp4_kept: bool,             // 不动
    pub bytes: u64,                 // 不动语义"实际写入字节数"
                                    //   旧 mp4 全下：= mp4 大小
                                    //   V5.D audio-only：= m4a 大小（单一文件主产物字节）
    pub elapsed_ms: u128,
    pub mp4_url_redacted: String,
    // V5.D NEW（additive）
    pub download_kind: String,      // "mp4-full" | "m4a-direct"
                                    //   旧消费方不读则忽略；新消费方靠它分流
    pub bytes_downloaded: u64,      // 实际从 CDN 拉的字节数
                                    //   "mp4-full" = bytes（mp4 大小）
                                    //   "m4a-direct" ≈ moov(~1MB) + audio samples + Range merge gap
                                    //                  通常 略大于 bytes 5-10%
}
```

`DownloadData`（单讲单 channel envelope，`data.rs:57-83`）：同样追加 `download_kind: String` + `bytes_downloaded: u64`。

`BatchData`（批量 envelope，`data.rs:120-143`）：追加 `total_bytes_downloaded: u64`，原 `total_bytes` 保留含义不变（落盘累计字节）。

`BatchEntry` / `DownloadAllData`：复用 `ChannelOutput`，自动跟随。

**旧消费方影响**：未知字段被 serde / yaml / jq 自动忽略，零改动 100% 兼容。

**新消费方收益**：
- `download_kind` 一眼区分入口（不必靠扩展名 / 字节数大小猜）
- `bytes_downloaded` 让 V5.D Phase 2 9-vs-9 对比能直接出"网络节省比"，不必反推
- yt-dlp 风格的多字段分语义，未来加新下载入口（如 cdn-cache-replay）也是 additive 加 `download_kind` 枚举值

**file_path 占位 quirk 保留**：V4 既有"audio-only 删 mp4 后 file_path 仍填 mp4 路径"的 quirk 保留不动（避免破坏 V4 已发布消费方）。新消费方靠 `audio_path` + `download_kind` 知道实际能打开的文件是 m4a。

CHANGELOG 写入 `tasks/lessons.md` 的 V5.D 章节明确这两条新字段的含义。

## 行数预算（每文件 < 200 硬限）

| 文件 | 状态 | 估行 |
|---|---|---|
| `apps/canvas_video/mp4_box/mod.rs` | NEW | ~30 |
| `apps/canvas_video/mp4_box/parser.rs` | NEW | ~180 |
| `apps/canvas_video/mp4_box/tests.rs` | NEW | ~120 |
| `apps/canvas_video/m4a_mux/mod.rs` | NEW | ~150 |
| `apps/canvas_video/m4a_mux/tests.rs` | NEW | ~100 |
| `apps/canvas_video/audio_dl/mod.rs` | NEW | ~30 |
| `apps/canvas_video/audio_dl/client.rs` | NEW | ~80 |
| `apps/canvas_video/audio_dl/orchestrator.rs` | NEW | ~180 |
| `apps/canvas_video/audio_dl/tests.rs` | NEW | ~120 |
| `apps/canvas_video/mod.rs` | +3 | 33 |
| `apps/canvas_video/download.rs` | 0 | 200（不动） |
| `commands/canvas_video/download_shared.rs` | +~10（audio_only 分支 + 填新字段） | ~80 |
| `commands/canvas_video/data.rs` | +~10（DownloadData / ChannelOutput / BatchData 新字段 + 注释） | ~178 |

3 个 NEW 模块各拆子文件，总新增 ~990 行。最大单文件 mp4_box/parser.rs 180 行，留 20 行余量。

## 测试策略

### 单元测试（mockito + fixture）

1. **mp4_box::tests**
   - `parse_moov_faststart`：用 ffmpeg 生成 `ftyp+moov+mdat` 排列的 1 min 假 mp4，解析出 AudioTrack
   - `parse_moov_standard`：`ftyp+mdat+moov` 排列，解析正确
   - `parse_unknown_codec`：non-AAC mp4 → 报 codec 不支持错误
   - `parse_corrupted_box`：坏 box size → graceful Err

2. **m4a_mux::tests**
   - `write_m4a_round_trip`：parse → mux → 再 parse 应等价
   - `m4a_ffprobe_valid`：mux 出的 m4a 跑 `ffprobe -v quiet -print_format json -show_streams` 解析成功，codec=aac

3. **audio_dl::tests**
   - `inter_byte_timeout_aborts`：mockito 模拟连接 OK 但 chunk 间 60 s 无字节 → 在 30 s 触发 abort + retry
   - `range_merge_minimizes_requests`：3000 sample 输入，合并后 < 100 Range request
   - `moov_in_tail_fallback`：mockito 第一段头部不含 moov，自动尾部 fetch

### 真机验证（V5.D Phase 2）

跑后 9 讲（L10-L18）：
```bash
sjtu canvas-video download 88168 --lectures 10-18 --channel 0 --audio-only --to tmp/v5_audio_18 --yaml > tmp/v5_audio_18/_v5d.stdout.log 2>tmp/v5_audio_18/_v5d.stderr.log
```

**对比维度**（vs 基线）：
- **网络字节节省**（V5.D 主指标）：直接读新 envelope 的 `bytes_downloaded` 累加 vs 基线 9 讲 mp4 size 累加。预期 ~38× 节省（~7.5 GB → ~200 MB）
- **总墙钟**：旧 sustained 21 min/讲 = 189 min / 新预期 3-5 min/讲 = 27-45 min（~5× 提速）
- **m4a 主产物完整性**：ffprobe 验 9 个新 m4a：codec=aac，duration 与基线对应讲对比 < 1 秒误差，bitrate 一致
- **envelope 字段对照**：新 envelope `download_kind == "m4a-direct"` × 9 / `bytes_downloaded` 远小于 `bytes`（mp4 旧路径 = bytes，新路径 ≈ bytes × 1.05-1.10）
- **抽样 m4a 与基线对比**：随机抽 3 讲（如 L11 / L14 / L17）的新 m4a vs 基线对应讲 m4a，文件大小差 < 5%（音频流应等价）

## 验证（4 关）

1. `cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` / `cargo test --lib`：全绿
2. `cargo run --release -- canvas-video download 88168 --lecture 10 --channel 0 --audio-only --to tmp/v5d_smoke --yaml`：单讲 audio-only smoke，envelope `bytes` < 30 MB，`audio_path` 存在，ffprobe 通
3. 同条件二跑：skip 路径生效（V4 行为不被破坏）
4. V5.D Phase 2 真机：后 9 讲全跑，envelope `succeeded == 9`，与基线对比报告写到 `tasks/lessons.md`

## 风险与缓解

- **风险**：mp4 codec 非 mp4a/AAC（如 Opus）→ AudioTrack codec 字段保留，mux 用相同 stsd → 通用；不支持的 codec 在 parse 阶段报错
- **风险**：moov 尾部找 box 翻倍 fetch 仍找不到 → fail-soft 回退到旧 mp4 全下路径（同 envelope 输出，bytes 自动是 mp4 字节）
- **风险**：m4a mux 出 ffprobe 不认的容器 → 单测用 ffprobe 把关，CI 不跑（CI 没 ffmpeg）但本地必跑
- **风险**：90 s 段 timeout 误伤合法慢响应 → 90 s vs 旧 30 min；audio-only 单段 < 5 MB，CDN 1 MB/s 限速也只要 5 s，10× 安全垫
- **风险**：30 s inter-byte 误伤断断续续的合法 chunk → 30 s 远长于正常 TCP 重传（< 1 s）；测试覆盖正常 chunk 场景
- **风险**：双路径分支让 download_shared.rs 复杂化 → 范围严格隔离：if audio_only call new path else old；3 行分支
- **风险**：V5.D 上后用户旧脚本依赖 `bytes` = mp4 大小 → CHANGELOG 明确告示；ChannelOutput 注释更新

## 决策记录（brainstorming）

| 决策点 | 选择 | brainstorming 答案位置 |
|---|---|---|
| mp4 parser | 手写最小化 | Q1 = "手写最小 parser（推荐）" |
| 范围 | 仅 audio-only 路径 | Q2 = "只优化 audio-only 路径（推荐）" |
| read_timeout | 90 s 段级 + 30 s inter-byte | Q3 = "90s 段级 + 30s inter-byte（推荐）" |
| 工作流 | spec → plan → subagent | Q4 = "spec→plan→subagent-driven（推荐）" |
| envelope 兼容性 | additive：加 `download_kind` + `bytes_downloaded` 不动旧字段 | 联网交叉验证（Confluent / Conduktor / yt-dlp 三家共识）"改语义 = breaking，加字段 = 安全" |

## 整体执行序

```
[V5.D spec 用户复核 ⏳]
   ↓
[invoke writing-plans 出 V5.D 实装计划（10-12 task）]
   ↓
[subagent-driven-development 逐 task 执行 + 两阶段 review]
   ↓
[V5.D 单元测试全绿 + cargo clippy + fmt + lint 4 关]
   ↓
[V5.D Phase 2 真机：后 9 讲（L10-L18）+ 与基线对比]
   ↓
[V5.D commit + V5.B Phase 2 收口 + tasks/lessons.md 更新]
```
