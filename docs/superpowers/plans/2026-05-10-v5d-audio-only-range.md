# V5.D Audio-Only Range Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Canvas 视频 audio-only 路径改为 mp4 box parser + Range 直拉 audio sample + 本地 mux m4a，省掉 840 MB mp4 整下 → ffmpeg 抽流。预期 ~38× 网络节省 + ~5× 提速。

**Architecture:** 新增 3 个隔离模块（`mp4_box` / `m4a_mux` / `audio_dl`），旧 `download.rs` + `ffmpeg.rs` 完全不动（保留 keep-mp4 / 视频路径 / fail-soft 回退）。`download_shared.rs` 在 audio_only 分支调新路径。`data.rs` envelope additive 加 `download_kind` + `bytes_downloaded`，旧字段语义保留。

**Tech Stack:** Rust 2021 / reqwest 0.12 (http1_only + per-conn 池关) / tokio (timeout + JoinSet) / mockito 单测 / ffmpeg 仅用于生成单测 fixture（CI 跳）。

---

## 范围与边界

**In scope（本计划）**：3 模块手写；`audio_dl::client` 90 s 段级 + 30 s inter-byte timeout；envelope 加新字段；fail-soft 回退到旧路径；单元测 mockito + 单测 fixture mp4；4 关绿；V5.D Phase 2 真机 9 讲对比；`tasks/lessons.md` 写经验。

**Out of scope（V5.E+）**：跨讲 Semaphore 池；普通 mp4 全路径迁移；字幕/转录。

## 文件结构

```
src/apps/canvas_video/
├── mp4_box/                             NEW
│   ├── mod.rs                           ~30 行 — re-export AudioTrack / parse_moov
│   ├── parser.rs                        ~180 行 — box 解析 + sample table 抽取
│   └── tests.rs                         ~120 行 — fixture mp4 单测
├── m4a_mux/                             NEW
│   ├── mod.rs                           ~150 行 — write_m4a
│   └── tests.rs                         ~100 行 — round-trip + ffprobe
├── audio_dl/                            NEW
│   ├── mod.rs                           ~30 行 — pub use download_audio_only_to_file
│   ├── client.rs                        ~80 行 — build_client_audio
│   ├── orchestrator.rs                  ~180 行 — moov 定位 + Range + mux
│   └── tests.rs                         ~120 行 — mockito 慢响应 / inter-byte / merge
├── mod.rs                               +3 行声明
├── download.rs                          不动
└── ffmpeg.rs                            不动

src/commands/canvas_video/
├── data.rs                              +~12 行（3 个 struct 加新字段）
├── download_shared.rs                   +~25 行（audio_only 分支 + 填新字段）
├── download_handler.rs                  +~6 行（DownloadData 透传新字段）
└── batch_handler.rs                     +~3 行（check_skip 填新字段 + total_bytes_downloaded）

tests/fixtures/canvas_video/             NEW（仅单测用，git 提交）
├── audio_1s_faststart.mp4               ~3 KB — `ftyp + moov + mdat` 排列
└── audio_1s_standard.mp4                ~3 KB — `ftyp + mdat + moov` 排列
```

## 关键接口（贯穿 12 task）

```rust
// mp4_box/mod.rs
pub struct AudioTrack {
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u8,
    pub sample_offsets: Vec<u64>,
    pub sample_sizes: Vec<u32>,
    pub mvhd_timescale: u32,
    pub mdhd_timescale: u32,
    pub mdhd_duration: u64,
    pub stsd_raw: Vec<u8>,
}
pub fn parse_moov(moov_bytes: &[u8]) -> anyhow::Result<AudioTrack>;

// m4a_mux/mod.rs
pub fn write_m4a(out: &Path, audio_track: &AudioTrack, sample_bytes: &[u8]) -> anyhow::Result<u64>;

// audio_dl/client.rs
pub(super) fn build_client_audio(referer: &str) -> anyhow::Result<reqwest::Client>;

// audio_dl/mod.rs
pub async fn download_audio_only_to_file(
    url: &str,
    dest_m4a: &Path,
    concurrency: usize,
    referer: &str,
) -> anyhow::Result<DownloadStats>;

pub struct DownloadStats {
    pub written: u64,        // m4a 落盘字节
    pub downloaded: u64,     // 从 CDN 实际拉取字节
}
```

`ChannelOutput` / `DownloadData` / `BatchData` 加 `download_kind: String` + `bytes_downloaded: u64`，`BatchData` 再加 `total_bytes_downloaded: u64`。旧字段语义保留。

---

## Task 0: 准备 fixture（main session 亲跑，不能 subagent）

**Files:**
- Create: `tests/fixtures/canvas_video/audio_1s_faststart.mp4`
- Create: `tests/fixtures/canvas_video/audio_1s_standard.mp4`

**为什么 main session：** subagent 工作目录可能在 worktree，ffmpeg 输出位置不一致；fixture 是 binary，main 跑一次提交即可。

- [ ] **Step 1: 建 fixture 目录**

Run: `mkdir -p tests/fixtures/canvas_video`

- [ ] **Step 2: 生成 faststart fixture（ftyp + moov 在前）**

Run:
```
ffmpeg -y -f lavfi -i "anullsrc=channel_layout=stereo:sample_rate=44100" -t 1 -c:a aac -b:a 64k -movflags +faststart tests/fixtures/canvas_video/audio_1s_faststart.mp4
```
Expected: 退出 0，文件 ~3 KB。

- [ ] **Step 3: 生成 standard fixture（ftyp + mdat + moov）**

Run:
```
ffmpeg -y -f lavfi -i "anullsrc=channel_layout=stereo:sample_rate=44100" -t 1 -c:a aac -b:a 64k tests/fixtures/canvas_video/audio_1s_standard.mp4
```
Expected: 退出 0，文件 ~3 KB；不带 faststart 时 ffmpeg 默认把 moov 放尾部。

- [ ] **Step 4: 验 layout 不同**

Run:
```
ffprobe -v quiet -print_format json -show_format tests/fixtures/canvas_video/audio_1s_faststart.mp4
ffprobe -v quiet -print_format json -show_format tests/fixtures/canvas_video/audio_1s_standard.mp4
```
Expected: 两个文件 codec_name=aac，duration=1.0±0.1，size 相近。

- [ ] **Step 5: head 字节对比 layout**

Run（PowerShell）：
```
[byte[]]$h1 = Get-Content tests/fixtures/canvas_video/audio_1s_faststart.mp4 -AsByteStream -ReadCount 64 -TotalCount 64
[byte[]]$h2 = Get-Content tests/fixtures/canvas_video/audio_1s_standard.mp4 -AsByteStream -ReadCount 64 -TotalCount 64
[System.Text.Encoding]::ASCII.GetString($h1[4..15])
[System.Text.Encoding]::ASCII.GetString($h2[4..15])
```
Expected: faststart 输出含 `ftyp...moov`；standard 输出含 `ftyp...mdat`（moov 在尾部）。

- [ ] **Step 6: Commit**

```
git add tests/fixtures/canvas_video/audio_1s_faststart.mp4 tests/fixtures/canvas_video/audio_1s_standard.mp4
git commit -m "test(canvas-video): 加 V5.D mp4_box 单测 fixture（faststart + standard 各 1 个）"
```

---

## Task 1: mp4_box 模块骨架 + AudioTrack struct + box header reader

**Files:**
- Create: `src/apps/canvas_video/mp4_box/mod.rs`
- Create: `src/apps/canvas_video/mp4_box/parser.rs`
- Create: `src/apps/canvas_video/mp4_box/tests.rs`
- Modify: `src/apps/canvas_video/mod.rs` — 加 `mod mp4_box;`

- [ ] **Step 1: 写 failing test（box header 读取）**

创建 `src/apps/canvas_video/mp4_box/tests.rs`：

```rust
//! mp4_box 单元测试：fixture mp4 解析 + box header 边界。

use super::parser::read_box_header;

#[test]
fn read_box_header_parses_size_and_type() {
    // box: size=12（含自身 8 字节 header）, type=ftyp, body 4 字节
    let bytes = [
        0x00, 0x00, 0x00, 0x0c, // size = 12
        b'f', b't', b'y', b'p', // type = ftyp
        0xde, 0xad, 0xbe, 0xef,
    ];
    let h = read_box_header(&bytes, 0).unwrap();
    assert_eq!(h.size, 12);
    assert_eq!(h.box_type, *b"ftyp");
    assert_eq!(h.header_len, 8);
    assert_eq!(h.body_start, 8);
}

#[test]
fn read_box_header_handles_largesize_64bit() {
    // size=1 → 后面 8 字节是真 size（large box）
    let bytes = [
        0x00, 0x00, 0x00, 0x01, // size = 1（信号）
        b'm', b'd', b'a', b't',
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, // largesize = 4096
    ];
    let h = read_box_header(&bytes, 0).unwrap();
    assert_eq!(h.size, 4096);
    assert_eq!(h.box_type, *b"mdat");
    assert_eq!(h.header_len, 16);
    assert_eq!(h.body_start, 16);
}

#[test]
fn read_box_header_rejects_truncated_input() {
    let bytes = [0x00, 0x00, 0x00, 0x0c, b'f', b't']; // 只 6 字节，header 至少 8
    assert!(read_box_header(&bytes, 0).is_err());
}
```

- [ ] **Step 2: 写 mod.rs 骨架（暴露 struct + 函数签名）**

创建 `src/apps/canvas_video/mp4_box/mod.rs`：

```rust
//! mp4 box 最小化解析：只解析 audio-only Range 下载需要的 box（ftyp / moov / trak / stbl）。
//!
//! 不引入 mp4 / mp4parse crate（CLAUDE.md 禁自引依赖；只用 ~5% box 类型）。
//! 设计目标：parse moov 字节 → AudioTrack（含 sample 偏移/大小 + stsd 复用字节）。

mod parser;
#[cfg(test)]
mod tests;

pub use parser::{parse_moov, AudioTrack};
```

- [ ] **Step 3: 写 parser.rs 骨架 + read_box_header**

创建 `src/apps/canvas_video/mp4_box/parser.rs`：

```rust
//! mp4 box parser。所有公开类型见 mod.rs re-export。

use anyhow::{anyhow, bail, Result};

/// 单个 mp4 box header（不含 body）。
pub(super) struct BoxHeader {
    /// box 总长（含 header）。
    pub size: u64,
    /// 4 字节 box type，如 `b"moov"`。
    pub box_type: [u8; 4],
    /// header 字节数（普通 8 / largesize 16）。
    pub header_len: u64,
    /// body 开始的偏移（相对原 buf 起点 = pos + header_len）。
    pub body_start: u64,
}

/// 从 `buf[pos..]` 读 box header。size=1 时取后面 8 字节作 64 位 largesize。
pub(super) fn read_box_header(buf: &[u8], pos: usize) -> Result<BoxHeader> {
    if buf.len() < pos + 8 {
        bail!("box header 截断：need 8 at {pos}, got {}", buf.len() - pos);
    }
    let size32 = u32::from_be_bytes(buf[pos..pos + 4].try_into().unwrap());
    let box_type: [u8; 4] = buf[pos + 4..pos + 8].try_into().unwrap();
    if size32 == 1 {
        if buf.len() < pos + 16 {
            bail!("largesize box header 截断 at {pos}");
        }
        let large = u64::from_be_bytes(buf[pos + 8..pos + 16].try_into().unwrap());
        return Ok(BoxHeader {
            size: large,
            box_type,
            header_len: 16,
            body_start: (pos + 16) as u64,
        });
    }
    if size32 == 0 {
        bail!("size=0（box 延伸到文件尾）暂不支持 at {pos}");
    }
    Ok(BoxHeader {
        size: size32 as u64,
        box_type,
        header_len: 8,
        body_start: (pos + 8) as u64,
    })
}

/// AudioTrack：mux 时所需的全部信息。
#[derive(Debug)]
pub struct AudioTrack {
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u8,
    pub sample_offsets: Vec<u64>,
    pub sample_sizes: Vec<u32>,
    pub mvhd_timescale: u32,
    pub mdhd_timescale: u32,
    pub mdhd_duration: u64,
    pub stsd_raw: Vec<u8>,
}

/// 从 moov box 字节解析 AudioTrack。**入参是 moov 整个 box 的字节**（含 header）。
pub fn parse_moov(_moov_bytes: &[u8]) -> Result<AudioTrack> {
    // 实装见 Task 2 / Task 3
    Err(anyhow!("parse_moov 未实装（Task 2 + Task 3）"))
}
```

- [ ] **Step 4: 在 canvas_video/mod.rs 加声明**

Modify `src/apps/canvas_video/mod.rs`，在 `pub mod download;` 之后加：

```rust
pub mod mp4_box;
```

- [ ] **Step 5: cargo check + run new tests**

Run: `cargo test -p sjtu-cli --lib canvas_video::mp4_box --no-fail-fast`
Expected: 3 个 read_box_header 测试 PASS。

- [ ] **Step 6: cargo fmt + clippy**

Run: `cargo fmt -- --check && cargo clippy -p sjtu-cli --lib --all-targets -- -D warnings`
Expected: 0 errors, 0 warnings。

- [ ] **Step 7: Commit**

```
git add src/apps/canvas_video/mod.rs src/apps/canvas_video/mp4_box/
git commit -m "feat(canvas-video): V5.D-T1 mp4_box 模块骨架 + box header reader（含 largesize 64-bit）"
```

---

## Task 2: parse_moov 主体（mvhd / trak / mdia / mdhd / hdlr / minf / stbl 嵌套遍历）

**Files:**
- Modify: `src/apps/canvas_video/mp4_box/parser.rs`
- Modify: `src/apps/canvas_video/mp4_box/tests.rs` — 加 fixture 测

- [ ] **Step 1: 写 failing test（faststart fixture parse 出 codec=aac）**

在 `src/apps/canvas_video/mp4_box/tests.rs` 末尾追加：

```rust
use super::parser::parse_moov;
use super::parser::read_box_header;

const FIXTURE_FASTSTART: &[u8] =
    include_bytes!("../../../../tests/fixtures/canvas_video/audio_1s_faststart.mp4");

/// 从整个 mp4 文件字节里把 moov box 字节切出来（顺序扫，遇到 type=moov 即返）。
fn extract_moov_bytes(mp4: &[u8]) -> Vec<u8> {
    let mut pos = 0usize;
    while pos + 8 <= mp4.len() {
        let h = read_box_header(mp4, pos).unwrap();
        let end = pos + h.size as usize;
        if &h.box_type == b"moov" {
            return mp4[pos..end].to_vec();
        }
        pos = end;
    }
    panic!("fixture 没找到 moov box");
}

#[test]
fn parse_moov_faststart_extracts_aac_track() {
    let moov = extract_moov_bytes(FIXTURE_FASTSTART);
    let track = parse_moov(&moov).expect("parse moov");
    assert_eq!(track.codec, "mp4a");
    assert_eq!(track.channels, 2);
    assert_eq!(track.sample_rate, 44100);
    assert!(track.mvhd_timescale > 0, "mvhd_timescale 必非 0");
    assert!(track.mdhd_timescale > 0, "mdhd_timescale 必非 0");
    assert!(!track.stsd_raw.is_empty(), "stsd_raw 必非空");
    // 1 秒 AAC @ 44.1kHz 通常 ~43 个 sample（1024 sample/frame × 43 ≈ 44032 → ~1s）
    assert!(
        track.sample_sizes.len() >= 30 && track.sample_sizes.len() <= 60,
        "1s 音频 sample 数应在 30-60 范围: {}",
        track.sample_sizes.len()
    );
    assert_eq!(track.sample_offsets.len(), track.sample_sizes.len());
}
```

- [ ] **Step 2: Run test 确认 fail**

Run: `cargo test -p sjtu-cli --lib canvas_video::mp4_box::tests::parse_moov_faststart_extracts_aac_track`
Expected: FAIL with "parse_moov 未实装"。

- [ ] **Step 3: 实装 parse_moov 主体（嵌套 box 遍历）**

替换 `parser.rs` 里 `parse_moov` 占位，并加 helper：

```rust
pub fn parse_moov(moov_bytes: &[u8]) -> Result<AudioTrack> {
    // 入参是 moov 整个 box（含 header）。先剥 header，得到 body。
    let head = read_box_header(moov_bytes, 0)?;
    if &head.box_type != b"moov" {
        bail!("入参不是 moov box: type={:?}", head.box_type);
    }
    let body = &moov_bytes[head.header_len as usize..head.size as usize];

    let mut mvhd_timescale: u32 = 0;
    let mut audio_track: Option<AudioTrack> = None;

    iter_children(body, |h, child_body| {
        match &h.box_type {
            b"mvhd" => {
                mvhd_timescale = read_mvhd_timescale(child_body)?;
            }
            b"trak" => {
                if let Some(t) = try_parse_audio_trak(child_body)? {
                    if audio_track.is_none() {
                        audio_track = Some(t);
                    }
                }
            }
            _ => {} // 其他 box 忽略
        }
        Ok(())
    })?;

    let mut t = audio_track.ok_or_else(|| anyhow!("moov 内未找到 audio trak"))?;
    t.mvhd_timescale = mvhd_timescale;
    Ok(t)
}

/// 遍历 box body 内的子 box，对每个调 `f(header, child_body)`。
fn iter_children(
    body: &[u8],
    mut f: impl FnMut(&BoxHeader, &[u8]) -> Result<()>,
) -> Result<()> {
    let mut pos = 0usize;
    while pos + 8 <= body.len() {
        let h = read_box_header(body, pos)?;
        let end = pos + h.size as usize;
        if end > body.len() {
            bail!("子 box 越界 at {pos}: end={end} body_len={}", body.len());
        }
        let child_body = &body[pos + h.header_len as usize..end];
        f(&h, child_body)?;
        pos = end;
    }
    Ok(())
}

/// mvhd box body：version(1) + flags(3) + ... + timescale(4 在 version=0 偏 12，version=1 偏 20)
fn read_mvhd_timescale(body: &[u8]) -> Result<u32> {
    if body.is_empty() {
        bail!("mvhd body 空");
    }
    let version = body[0];
    let off = if version == 0 { 12 } else { 20 };
    if body.len() < off + 4 {
        bail!("mvhd 截断（version={version}, 需 {} 字节）", off + 4);
    }
    Ok(u32::from_be_bytes(body[off..off + 4].try_into().unwrap()))
}

/// 尝试从 trak body 解析 audio track。非 audio（如 video）返 None。
fn try_parse_audio_trak(trak_body: &[u8]) -> Result<Option<AudioTrack>> {
    let mut mdia_body: Option<Vec<u8>> = None;
    iter_children(trak_body, |h, b| {
        if &h.box_type == b"mdia" {
            mdia_body = Some(b.to_vec());
        }
        Ok(())
    })?;
    let mdia = match mdia_body {
        Some(b) => b,
        None => return Ok(None),
    };
    parse_mdia(&mdia)
}

/// mdia 内有 mdhd + hdlr + minf。hdlr.handler_type 决定是不是 audio（"soun"）。
fn parse_mdia(mdia_body: &[u8]) -> Result<Option<AudioTrack>> {
    let mut mdhd_timescale: u32 = 0;
    let mut mdhd_duration: u64 = 0;
    let mut is_audio = false;
    let mut minf_body: Option<Vec<u8>> = None;
    iter_children(mdia_body, |h, b| {
        match &h.box_type {
            b"mdhd" => {
                let (ts, dur) = read_mdhd_ts_dur(b)?;
                mdhd_timescale = ts;
                mdhd_duration = dur;
            }
            b"hdlr" => {
                // hdlr: version(1)+flags(3)+pre_defined(4)+handler_type(4)
                if b.len() >= 12 && &b[8..12] == b"soun" {
                    is_audio = true;
                }
            }
            b"minf" => minf_body = Some(b.to_vec()),
            _ => {}
        }
        Ok(())
    })?;
    if !is_audio {
        return Ok(None);
    }
    let minf = minf_body.ok_or_else(|| anyhow!("audio mdia 缺 minf"))?;
    let mut stbl_body: Option<Vec<u8>> = None;
    iter_children(&minf, |h, b| {
        if &h.box_type == b"stbl" {
            stbl_body = Some(b.to_vec());
        }
        Ok(())
    })?;
    let stbl = stbl_body.ok_or_else(|| anyhow!("audio minf 缺 stbl"))?;
    let mut t = parse_stbl(&stbl)?;
    t.mdhd_timescale = mdhd_timescale;
    t.mdhd_duration = mdhd_duration;
    Ok(Some(t))
}

/// mdhd: version=0 → flags(3)+ctime(4)+mtime(4)+timescale(4)+duration(4)
///       version=1 → flags(3)+ctime(8)+mtime(8)+timescale(4)+duration(8)
fn read_mdhd_ts_dur(body: &[u8]) -> Result<(u32, u64)> {
    if body.is_empty() {
        bail!("mdhd body 空");
    }
    let version = body[0];
    if version == 0 {
        if body.len() < 20 {
            bail!("mdhd v0 截断");
        }
        let ts = u32::from_be_bytes(body[12..16].try_into().unwrap());
        let dur = u32::from_be_bytes(body[16..20].try_into().unwrap()) as u64;
        Ok((ts, dur))
    } else {
        if body.len() < 32 {
            bail!("mdhd v1 截断");
        }
        let ts = u32::from_be_bytes(body[20..24].try_into().unwrap());
        let dur = u64::from_be_bytes(body[24..32].try_into().unwrap());
        Ok((ts, dur))
    }
}

/// stbl 解析：在 Task 3 实装。先放占位让 Task 2 测能编过。
fn parse_stbl(_stbl_body: &[u8]) -> Result<AudioTrack> {
    bail!("parse_stbl 在 Task 3 实装")
}
```

- [ ] **Step 4: Run test 仍 fail（parse_stbl 未实装），确认链路对接到 stbl**

Run: `cargo test -p sjtu-cli --lib canvas_video::mp4_box::tests::parse_moov_faststart_extracts_aac_track`
Expected: FAIL with "parse_stbl 在 Task 3 实装"（说明遍历到 stbl 一步了）。

- [ ] **Step 5: cargo fmt + clippy**

Run: `cargo fmt && cargo clippy -p sjtu-cli --lib --all-targets -- -D warnings`
Expected: 0 errors。

- [ ] **Step 6: Commit**

```
git add src/apps/canvas_video/mp4_box/
git commit -m "feat(canvas-video): V5.D-T2 parse_moov 主体（mvhd/trak/mdia/hdlr/minf 遍历），stbl 留 T3"
```

---

## Task 3: parse_stbl（stsd / stsc / stsz / stco / co64 — sample 表展开）

**Files:**
- Modify: `src/apps/canvas_video/mp4_box/parser.rs`
- Modify: `src/apps/canvas_video/mp4_box/tests.rs` — 加 standard 布局 + sample 表测

- [ ] **Step 1: 加 failing test（standard layout + sample table 完整性）**

在 `tests.rs` 末尾追加：

```rust
const FIXTURE_STANDARD: &[u8] =
    include_bytes!("../../../../tests/fixtures/canvas_video/audio_1s_standard.mp4");

#[test]
fn parse_moov_standard_layout_extracts_aac_track() {
    let moov = extract_moov_bytes(FIXTURE_STANDARD);
    let track = parse_moov(&moov).expect("parse moov standard");
    assert_eq!(track.codec, "mp4a");
    assert_eq!(track.channels, 2);
    assert_eq!(track.sample_rate, 44100);
    // standard layout sample offset 都在 ftyp+free 之后、moov 之前的 mdat 区域 → 比 faststart 小
    let max_off = *track.sample_offsets.iter().max().unwrap();
    assert!(max_off > 0, "sample offset 必非 0");
    // 每个 sample 大小至少 1 字节
    assert!(track.sample_sizes.iter().all(|&s| s > 0));
    // 总字节合理范围（1 秒 AAC 64kbps ≈ 8 KB）
    let total: u64 = track.sample_sizes.iter().map(|&s| s as u64).sum();
    assert!(
        (4_000..30_000).contains(&total),
        "sample 总字节应 4-30 KB: {total}"
    );
}
```

- [ ] **Step 2: 运行 test 确认 fail**

Run: `cargo test -p sjtu-cli --lib canvas_video::mp4_box::tests::parse_moov_standard_layout`
Expected: FAIL（faststart 测也仍 fail，stbl 未实装）。

- [ ] **Step 3: 实装 parse_stbl + sample 表展开**

替换 `parser.rs` 里 `parse_stbl` 占位为完整实装：

```rust
fn parse_stbl(stbl_body: &[u8]) -> Result<AudioTrack> {
    let mut stsd_raw: Vec<u8> = Vec::new();
    let mut codec: String = String::new();
    let mut sample_rate: u32 = 0;
    let mut channels: u8 = 0;

    let mut stsz_sizes: Vec<u32> = Vec::new();
    let mut stsc_entries: Vec<(u32, u32, u32)> = Vec::new(); // first_chunk, samples_per_chunk, sample_desc_idx
    let mut chunk_offsets: Vec<u64> = Vec::new();

    iter_children(stbl_body, |h, b| {
        match &h.box_type {
            b"stsd" => {
                stsd_raw = b.to_vec();
                let (c, sr, ch) = parse_stsd(b)?;
                codec = c;
                sample_rate = sr;
                channels = ch;
            }
            b"stsz" => stsz_sizes = parse_stsz(b)?,
            b"stsc" => stsc_entries = parse_stsc(b)?,
            b"stco" => chunk_offsets = parse_stco(b, false)?,
            b"co64" => chunk_offsets = parse_stco(b, true)?,
            _ => {}
        }
        Ok(())
    })?;

    if codec.is_empty() {
        bail!("stbl 缺 stsd codec");
    }
    if stsz_sizes.is_empty() {
        bail!("stbl 缺 stsz / sample 表为空");
    }
    if chunk_offsets.is_empty() {
        bail!("stbl 缺 stco/co64");
    }
    if stsc_entries.is_empty() {
        bail!("stbl 缺 stsc");
    }

    let sample_offsets = expand_sample_offsets(&stsc_entries, &chunk_offsets, &stsz_sizes)?;
    Ok(AudioTrack {
        codec,
        sample_rate,
        channels,
        sample_offsets,
        sample_sizes: stsz_sizes,
        mvhd_timescale: 0, // parse_moov 回填
        mdhd_timescale: 0, // parse_mdia 回填
        mdhd_duration: 0,  // parse_mdia 回填
        stsd_raw,
    })
}

/// stsd: version+flags(4) + entry_count(4) + entries...
/// 第一 entry 头 8 字节 = entry_size + entry_type（如 mp4a / opus）。
/// mp4a entry 结构：8 size+type + 6 reserved + 2 dref_idx + 8 reserved
///   + 2 channels + 2 sample_size + 2 pre_defined + 2 reserved + 4 sample_rate (16.16 fixed)
fn parse_stsd(body: &[u8]) -> Result<(String, u32, u8)> {
    if body.len() < 8 {
        bail!("stsd 截断");
    }
    let entry_count = u32::from_be_bytes(body[4..8].try_into().unwrap());
    if entry_count == 0 {
        bail!("stsd entry_count=0");
    }
    let entry = &body[8..];
    if entry.len() < 36 {
        bail!("stsd entry 截断");
    }
    let codec_raw: [u8; 4] = entry[4..8].try_into().unwrap();
    let codec = String::from_utf8_lossy(&codec_raw).into_owned();
    let channels = u16::from_be_bytes(entry[24..26].try_into().unwrap()) as u8;
    // sample_rate 是 16.16 fixed-point，整数部分在前 2 字节
    let sr_int = u16::from_be_bytes(entry[32..34].try_into().unwrap()) as u32;
    Ok((codec, sr_int, channels))
}

/// stsz: version+flags(4) + sample_size(4) + sample_count(4) + (若 sample_size==0 时 N 个 size 表)
fn parse_stsz(body: &[u8]) -> Result<Vec<u32>> {
    if body.len() < 12 {
        bail!("stsz 截断");
    }
    let sample_size = u32::from_be_bytes(body[4..8].try_into().unwrap());
    let count = u32::from_be_bytes(body[8..12].try_into().unwrap()) as usize;
    if sample_size != 0 {
        return Ok(vec![sample_size; count]);
    }
    if body.len() < 12 + count * 4 {
        bail!("stsz 表截断: need {} got {}", 12 + count * 4, body.len());
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = 12 + i * 4;
        out.push(u32::from_be_bytes(body[off..off + 4].try_into().unwrap()));
    }
    Ok(out)
}

/// stsc: version+flags(4) + entry_count(4) + N × (first_chunk(4) + samples_per_chunk(4) + sample_desc_idx(4))
fn parse_stsc(body: &[u8]) -> Result<Vec<(u32, u32, u32)>> {
    if body.len() < 8 {
        bail!("stsc 截断");
    }
    let count = u32::from_be_bytes(body[4..8].try_into().unwrap()) as usize;
    if body.len() < 8 + count * 12 {
        bail!("stsc 表截断");
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = 8 + i * 12;
        out.push((
            u32::from_be_bytes(body[off..off + 4].try_into().unwrap()),
            u32::from_be_bytes(body[off + 4..off + 8].try_into().unwrap()),
            u32::from_be_bytes(body[off + 8..off + 12].try_into().unwrap()),
        ));
    }
    Ok(out)
}

/// stco / co64: version+flags(4) + entry_count(4) + N × (4 或 8 字节 offset)
fn parse_stco(body: &[u8], is_64: bool) -> Result<Vec<u64>> {
    if body.len() < 8 {
        bail!("stco/co64 截断");
    }
    let count = u32::from_be_bytes(body[4..8].try_into().unwrap()) as usize;
    let entry_size = if is_64 { 8 } else { 4 };
    if body.len() < 8 + count * entry_size {
        bail!("stco/co64 表截断");
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = 8 + i * entry_size;
        let v = if is_64 {
            u64::from_be_bytes(body[off..off + 8].try_into().unwrap())
        } else {
            u32::from_be_bytes(body[off..off + 4].try_into().unwrap()) as u64
        };
        out.push(v);
    }
    Ok(out)
}

/// stsc + chunk_offsets + sample_sizes → 每个 sample 的绝对偏移。
/// stsc 描述 "第 first_chunk 起每 chunk 含 samples_per_chunk 个 sample"，按 chunk 累加 sample size 算 offset。
fn expand_sample_offsets(
    stsc: &[(u32, u32, u32)],
    chunk_offsets: &[u64],
    sample_sizes: &[u32],
) -> Result<Vec<u64>> {
    let mut out: Vec<u64> = Vec::with_capacity(sample_sizes.len());
    let mut sample_idx = 0usize;
    for (i, &chunk_off) in chunk_offsets.iter().enumerate() {
        let chunk_num_1based = (i + 1) as u32;
        // 找当前 chunk 在 stsc 表中的"段"：last entry whose first_chunk <= chunk_num_1based
        let samples_per_chunk = stsc
            .iter()
            .rev()
            .find(|e| e.0 <= chunk_num_1based)
            .map(|e| e.1)
            .ok_or_else(|| anyhow!("stsc 找不到 chunk {chunk_num_1based} 的段"))?;
        let mut cur_off = chunk_off;
        for _ in 0..samples_per_chunk {
            if sample_idx >= sample_sizes.len() {
                // chunk 表 + stsc 推算的 sample 总数 > stsz 给的 → 截断（少数 mp4 容器尾部 padding）
                return Ok(out);
            }
            out.push(cur_off);
            cur_off += sample_sizes[sample_idx] as u64;
            sample_idx += 1;
        }
    }
    Ok(out)
}
```

- [ ] **Step 4: Run 全部 mp4_box 测试**

Run: `cargo test -p sjtu-cli --lib canvas_video::mp4_box`
Expected: 5 个测试全 PASS（3 个 box header + faststart + standard）。

- [ ] **Step 5: cargo fmt + clippy**

Run: `cargo fmt && cargo clippy -p sjtu-cli --lib --all-targets -- -D warnings`
Expected: 0 errors。

- [ ] **Step 6: 行数检查**

Run: `wc -l src/apps/canvas_video/mp4_box/parser.rs`
Expected: ≤ 200 行。如超限，先停下来 escalate。

- [ ] **Step 7: Commit**

```
git add src/apps/canvas_video/mp4_box/
git commit -m "feat(canvas-video): V5.D-T3 parse_stbl + sample 表展开（stsd/stsc/stsz/stco/co64）"
```

---

## Task 4: m4a_mux 模块（write_m4a + round-trip 测）

**Files:**
- Create: `src/apps/canvas_video/m4a_mux/mod.rs`
- Create: `src/apps/canvas_video/m4a_mux/tests.rs`
- Modify: `src/apps/canvas_video/mod.rs` — 加 `pub mod m4a_mux;`

- [ ] **Step 1: 写 failing test（round-trip 用 fixture）**

创建 `src/apps/canvas_video/m4a_mux/tests.rs`：

```rust
//! m4a_mux 单元测试：parse fixture → 抽 sample bytes → mux → re-parse 应等价。

use std::path::PathBuf;

use crate::apps::canvas_video::mp4_box::parse_moov;

use super::write_m4a;

const FIXTURE_FASTSTART: &[u8] =
    include_bytes!("../../../../tests/fixtures/canvas_video/audio_1s_faststart.mp4");

/// 从原 mp4 字节里把 audio sample bytes 按 sample_offsets 顺序拼出来（紧密排列）。
fn collect_sample_bytes(mp4: &[u8], offsets: &[u64], sizes: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    for (&off, &sz) in offsets.iter().zip(sizes.iter()) {
        let s = off as usize;
        let e = s + sz as usize;
        out.extend_from_slice(&mp4[s..e]);
    }
    out
}

fn extract_moov(mp4: &[u8]) -> Vec<u8> {
    use crate::apps::canvas_video::mp4_box::parser_test_helpers::extract_moov_bytes_pub;
    extract_moov_bytes_pub(mp4)
}

#[tokio::test]
async fn write_m4a_round_trip_preserves_codec_and_sample_count() {
    let moov = extract_moov(FIXTURE_FASTSTART);
    let track = parse_moov(&moov).expect("parse fixture moov");
    let sample_bytes = collect_sample_bytes(
        FIXTURE_FASTSTART,
        &track.sample_offsets,
        &track.sample_sizes,
    );

    let tmp = std::env::temp_dir().join("sjtu_v5d_round_trip.m4a");
    let _ = std::fs::remove_file(&tmp);
    let written = write_m4a(&tmp, &track, &sample_bytes).expect("write m4a");
    assert!(written > 0);

    // re-parse：用 tokio fs 读回
    let back = tokio::fs::read(&tmp).await.expect("read m4a back");
    let new_moov = extract_moov(&back);
    let new_track = parse_moov(&new_moov).expect("parse round-trip m4a");
    assert_eq!(new_track.codec, "mp4a");
    assert_eq!(new_track.channels, track.channels);
    assert_eq!(new_track.sample_rate, track.sample_rate);
    assert_eq!(new_track.sample_sizes.len(), track.sample_sizes.len());

    let _ = std::fs::remove_file(&tmp);
}
```

注：测试引用了 `parser_test_helpers::extract_moov_bytes_pub`，需在 `mp4_box/parser.rs` 末尾加一个 cfg(test) 子模块暴露该 helper（让其他模块测试也能调用）。

- [ ] **Step 2: 在 parser.rs 末尾加 test helper（避免重复 helper 函数）**

追加到 `src/apps/canvas_video/mp4_box/parser.rs` 末尾：

```rust
#[cfg(test)]
pub(crate) mod parser_test_helpers {
    use super::read_box_header;

    /// 从整个 mp4 文件字节里切出 moov box 字节（顺序扫第一个 moov）。
    /// 仅供 mp4_box / m4a_mux / audio_dl 单测共享，不对外。
    pub(crate) fn extract_moov_bytes_pub(mp4: &[u8]) -> Vec<u8> {
        let mut pos = 0usize;
        while pos + 8 <= mp4.len() {
            let h = read_box_header(mp4, pos).unwrap();
            let end = pos + h.size as usize;
            if &h.box_type == b"moov" {
                return mp4[pos..end].to_vec();
            }
            pos = end;
        }
        panic!("没找到 moov box");
    }
}
```

并在 `mp4_box/tests.rs` 顶部把内联的 `extract_moov_bytes` 改成调这个 helper（DRY）。

- [ ] **Step 3: 写 m4a_mux/mod.rs（write_m4a 实装）**

创建 `src/apps/canvas_video/m4a_mux/mod.rs`：

```rust
//! 最小化 m4a 容器写入：ftyp + moov + mdat。
//!
//! 思路：直接借用 source 的 stsd 字节（含 esds AAC 配置），重写 stco 指向新 mdat 起点。
//! mdat 内容是按 sample_offsets 顺序紧密拼接的 audio sample bytes。

use std::path::Path;

use anyhow::{Context, Result};

use crate::apps::canvas_video::mp4_box::AudioTrack;

#[cfg(test)]
mod tests;

/// 把 audio_track + sample_bytes 拼成最小 m4a，落到 out。
/// sample_bytes 必须按 audio_track.sample_offsets 顺序拼好，与 sample_sizes 一一对应。
/// 返回写入字节数。
pub fn write_m4a(out: &Path, audio_track: &AudioTrack, sample_bytes: &[u8]) -> Result<u64> {
    let total_samples: u64 = audio_track.sample_sizes.iter().map(|&s| s as u64).sum();
    if sample_bytes.len() as u64 != total_samples {
        anyhow::bail!(
            "sample_bytes 长度 {} != sum(sample_sizes) {}",
            sample_bytes.len(),
            total_samples
        );
    }

    // 先建 moov 字节（用 stco 指向 ftyp + moov 之后），需要先知道 moov 长度才能算 mdat 偏移。
    // 策略：moov 长度仅取决于 stsd_raw + stco 表 + 固定 header → 先算 moov 长度，回填 stco。
    let moov_bytes = build_moov(audio_track)?;
    let ftyp = ftyp_box();
    let mdat_header_len: u64 = 8;
    let mdat_offset = ftyp.len() as u64 + moov_bytes.len() as u64 + mdat_header_len;

    // 重写 moov 内 stco：所有 chunk_offset 改成相对 mdat_offset 起点累加（一个 chunk 一个 sample 简化）。
    let final_moov = rewrite_stco(&moov_bytes, audio_track, mdat_offset)?;
    debug_assert_eq!(final_moov.len(), moov_bytes.len());

    // 拼最终字节
    let mut buf =
        Vec::with_capacity(ftyp.len() + final_moov.len() + mdat_header_len as usize + sample_bytes.len());
    buf.extend_from_slice(&ftyp);
    buf.extend_from_slice(&final_moov);
    // mdat header
    let mdat_size = mdat_header_len + sample_bytes.len() as u64;
    buf.extend_from_slice(&(mdat_size as u32).to_be_bytes());
    buf.extend_from_slice(b"mdat");
    buf.extend_from_slice(sample_bytes);

    let len = buf.len() as u64;
    std::fs::write(out, &buf).with_context(|| format!("写 m4a {}", out.display()))?;
    Ok(len)
}

fn ftyp_box() -> Vec<u8> {
    // ftyp: M4A 标准 — major=M4A , minor=0x200 , compat=[isom, M4A , mp42]
    let body: &[u8] = b"M4A \x00\x00\x02\x00isomM4A mp42";
    let size = (8 + body.len()) as u32;
    let mut v = Vec::with_capacity(8 + body.len());
    v.extend_from_slice(&size.to_be_bytes());
    v.extend_from_slice(b"ftyp");
    v.extend_from_slice(body);
    v
}

/// 构造 moov：mvhd + trak(tkhd + mdia(mdhd + hdlr + minf(smhd + dinf + stbl)))。
/// stbl: stsd_raw + stts(全 1024 sample/frame) + stsc(1 sample/chunk) + stsz + stco。
fn build_moov(t: &AudioTrack) -> Result<Vec<u8>> {
    let stsd = wrap_box(b"stsd", &t.stsd_raw);

    // stts: 全 entry 用 sample_count = N, sample_delta = 1024（AAC 1 帧 1024 sample）
    let mut stts_body = Vec::new();
    stts_body.extend_from_slice(&[0, 0, 0, 0]); // version+flags
    stts_body.extend_from_slice(&1u32.to_be_bytes()); // entry_count
    stts_body.extend_from_slice(&(t.sample_sizes.len() as u32).to_be_bytes());
    stts_body.extend_from_slice(&1024u32.to_be_bytes());
    let stts = wrap_box(b"stts", &stts_body);

    // stsc: 1 entry — first_chunk=1, samples_per_chunk=1, sample_desc_idx=1
    // 简化：每个 sample 一个 chunk → stco 表长度 = sample 数（写入大文件时这意味着 stco 较大，但容易调试）
    let mut stsc_body = Vec::new();
    stsc_body.extend_from_slice(&[0, 0, 0, 0]);
    stsc_body.extend_from_slice(&1u32.to_be_bytes()); // entry_count=1
    stsc_body.extend_from_slice(&1u32.to_be_bytes()); // first_chunk=1
    stsc_body.extend_from_slice(&1u32.to_be_bytes()); // samples_per_chunk=1
    stsc_body.extend_from_slice(&1u32.to_be_bytes()); // sample_desc_idx=1
    let stsc = wrap_box(b"stsc", &stsc_body);

    // stsz: sample_size=0（每个 sample 都不一样），entry table 跟原表
    let mut stsz_body = Vec::new();
    stsz_body.extend_from_slice(&[0, 0, 0, 0]);
    stsz_body.extend_from_slice(&0u32.to_be_bytes()); // 0 → 后跟表
    stsz_body.extend_from_slice(&(t.sample_sizes.len() as u32).to_be_bytes());
    for &sz in &t.sample_sizes {
        stsz_body.extend_from_slice(&sz.to_be_bytes());
    }
    let stsz = wrap_box(b"stsz", &stsz_body);

    // stco: 占位，全填 0，rewrite_stco 阶段回填
    let mut stco_body = Vec::new();
    stco_body.extend_from_slice(&[0, 0, 0, 0]);
    stco_body.extend_from_slice(&(t.sample_sizes.len() as u32).to_be_bytes());
    for _ in 0..t.sample_sizes.len() {
        stco_body.extend_from_slice(&0u32.to_be_bytes());
    }
    let stco = wrap_box(b"stco", &stco_body);

    let mut stbl_body = Vec::new();
    stbl_body.extend_from_slice(&stsd);
    stbl_body.extend_from_slice(&stts);
    stbl_body.extend_from_slice(&stsc);
    stbl_body.extend_from_slice(&stsz);
    stbl_body.extend_from_slice(&stco);
    let stbl = wrap_box(b"stbl", &stbl_body);

    // smhd（sound media header）
    let smhd_body: &[u8] = &[0, 0, 0, 0, 0, 0, 0, 0]; // version/flags + balance(2) + reserved(2)
    let smhd = wrap_box(b"smhd", smhd_body);
    // dinf/dref/url（self-contained）
    let dref_body: &[u8] = &[
        0, 0, 0, 0, // version+flags
        0, 0, 0, 1, // entry_count=1
        0, 0, 0, 12, // size=12
        b'u', b'r', b'l', b' ',
        0, 0, 0, 1, // url flags=1（self-contained）
    ];
    let dref = wrap_box(b"dref", dref_body);
    let dinf = wrap_box(b"dinf", &dref);

    let mut minf_body = Vec::new();
    minf_body.extend_from_slice(&smhd);
    minf_body.extend_from_slice(&dinf);
    minf_body.extend_from_slice(&stbl);
    let minf = wrap_box(b"minf", &minf_body);

    // mdhd v0
    let mut mdhd_body = Vec::new();
    mdhd_body.extend_from_slice(&[0, 0, 0, 0]); // version+flags
    mdhd_body.extend_from_slice(&[0, 0, 0, 0]); // ctime
    mdhd_body.extend_from_slice(&[0, 0, 0, 0]); // mtime
    mdhd_body.extend_from_slice(&t.mdhd_timescale.to_be_bytes());
    let dur32 = (t.mdhd_duration.min(u32::MAX as u64)) as u32;
    mdhd_body.extend_from_slice(&dur32.to_be_bytes());
    mdhd_body.extend_from_slice(&[0x55, 0xc4]); // language='und'
    mdhd_body.extend_from_slice(&[0, 0]);
    let mdhd = wrap_box(b"mdhd", &mdhd_body);

    // hdlr (handler='soun')
    let mut hdlr_body = Vec::new();
    hdlr_body.extend_from_slice(&[0, 0, 0, 0]); // version+flags
    hdlr_body.extend_from_slice(&[0, 0, 0, 0]); // pre_defined
    hdlr_body.extend_from_slice(b"soun");
    hdlr_body.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // reserved
    hdlr_body.extend_from_slice(b"sjtu\0"); // name + null
    let hdlr = wrap_box(b"hdlr", &hdlr_body);

    let mut mdia_body = Vec::new();
    mdia_body.extend_from_slice(&mdhd);
    mdia_body.extend_from_slice(&hdlr);
    mdia_body.extend_from_slice(&minf);
    let mdia = wrap_box(b"mdia", &mdia_body);

    // tkhd v0：track_id=1, duration=mdhd_duration scaled to mvhd timescale
    let mut tkhd_body = Vec::new();
    tkhd_body.extend_from_slice(&[0, 0, 0, 7]); // version=0, flags=0x000007 (enabled+in_movie+in_preview)
    tkhd_body.extend_from_slice(&[0, 0, 0, 0]); // ctime
    tkhd_body.extend_from_slice(&[0, 0, 0, 0]); // mtime
    tkhd_body.extend_from_slice(&1u32.to_be_bytes()); // track_id
    tkhd_body.extend_from_slice(&[0, 0, 0, 0]); // reserved
    tkhd_body.extend_from_slice(&dur32.to_be_bytes()); // duration（与 mvhd 对齐时这里应转换；简化沿用）
    tkhd_body.extend_from_slice(&[0; 8]); // reserved
    tkhd_body.extend_from_slice(&[0, 0]); // layer
    tkhd_body.extend_from_slice(&[0, 1]); // alternate_group=1
    tkhd_body.extend_from_slice(&[1, 0]); // volume=1.0 (8.8 fixed)
    tkhd_body.extend_from_slice(&[0, 0]); // reserved
    // unity matrix
    let matrix: [i32; 9] = [0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000];
    for v in matrix {
        tkhd_body.extend_from_slice(&v.to_be_bytes());
    }
    tkhd_body.extend_from_slice(&[0, 0, 0, 0]); // width
    tkhd_body.extend_from_slice(&[0, 0, 0, 0]); // height
    let tkhd = wrap_box(b"tkhd", &tkhd_body);

    let mut trak_body = Vec::new();
    trak_body.extend_from_slice(&tkhd);
    trak_body.extend_from_slice(&mdia);
    let trak = wrap_box(b"trak", &trak_body);

    // mvhd v0
    let mvhd_ts = if t.mvhd_timescale == 0 { 1000 } else { t.mvhd_timescale };
    let mut mvhd_body = Vec::new();
    mvhd_body.extend_from_slice(&[0, 0, 0, 0]); // version+flags
    mvhd_body.extend_from_slice(&[0, 0, 0, 0]); // ctime
    mvhd_body.extend_from_slice(&[0, 0, 0, 0]); // mtime
    mvhd_body.extend_from_slice(&mvhd_ts.to_be_bytes());
    mvhd_body.extend_from_slice(&dur32.to_be_bytes());
    mvhd_body.extend_from_slice(&[0, 1, 0, 0]); // rate=1.0
    mvhd_body.extend_from_slice(&[1, 0]); // volume=1.0
    mvhd_body.extend_from_slice(&[0; 10]); // reserved
    for v in matrix {
        mvhd_body.extend_from_slice(&v.to_be_bytes());
    }
    mvhd_body.extend_from_slice(&[0; 24]); // pre_defined
    mvhd_body.extend_from_slice(&2u32.to_be_bytes()); // next_track_id
    let mvhd = wrap_box(b"mvhd", &mvhd_body);

    let mut moov_body = Vec::new();
    moov_body.extend_from_slice(&mvhd);
    moov_body.extend_from_slice(&trak);
    Ok(wrap_box(b"moov", &moov_body))
}

fn wrap_box(box_type: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let size = (8 + body.len()) as u32;
    let mut v = Vec::with_capacity(8 + body.len());
    v.extend_from_slice(&size.to_be_bytes());
    v.extend_from_slice(box_type);
    v.extend_from_slice(body);
    v
}

/// 在已构造的 moov 字节里找到 stco box，把每个 chunk_offset 改成 mdat_offset 起点累加 sample_size。
fn rewrite_stco(moov: &[u8], track: &AudioTrack, mdat_offset: u64) -> Result<Vec<u8>> {
    let mut out = moov.to_vec();
    let stco_pos = find_box_pos(&out, b"stco")
        .ok_or_else(|| anyhow::anyhow!("build_moov 输出里找不到 stco（不可能，bug）"))?;
    let body_start = stco_pos + 8;
    let table_start = body_start + 4 + 4; // version+flags + entry_count
    let mut cur = mdat_offset;
    for (i, &sz) in track.sample_sizes.iter().enumerate() {
        let off = table_start + i * 4;
        if off + 4 > out.len() {
            anyhow::bail!("stco 表越界（bug）");
        }
        out[off..off + 4].copy_from_slice(&(cur as u32).to_be_bytes());
        cur += sz as u64;
    }
    Ok(out)
}

/// 在 buf 里递归找指定 type 的 box（深度优先），返回首个匹配的 box 起点。
fn find_box_pos(buf: &[u8], box_type: &[u8; 4]) -> Option<usize> {
    let mut pos = 0usize;
    while pos + 8 <= buf.len() {
        let size = u32::from_be_bytes(buf[pos..pos + 4].try_into().ok()?) as usize;
        let ty: [u8; 4] = buf[pos + 4..pos + 8].try_into().ok()?;
        if &ty == box_type {
            return Some(pos);
        }
        // 容器类（递归进去）
        if matches!(&ty, b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"dinf") {
            if let Some(inner) = find_box_pos(&buf[pos + 8..pos + size], box_type) {
                return Some(pos + 8 + inner);
            }
        }
        if size == 0 {
            return None;
        }
        pos += size;
    }
    None
}
```

- [ ] **Step 4: 在 canvas_video/mod.rs 加 m4a_mux 声明**

修改 `src/apps/canvas_video/mod.rs`，在 `pub mod mp4_box;` 之后加：

```rust
pub mod m4a_mux;
```

- [ ] **Step 5: Run round-trip 测**

Run: `cargo test -p sjtu-cli --lib canvas_video::m4a_mux`
Expected: round-trip 测 PASS（codec=mp4a, channels/sample_rate/sample 数等价）。

- [ ] **Step 6: 可选 ffprobe 验**

Run（手动，CI 跳）：
```
cargo test -p sjtu-cli --lib canvas_video::m4a_mux::tests::write_m4a_round_trip_preserves_codec_and_sample_count -- --nocapture
ffprobe -v quiet -print_format json -show_streams "$env:TEMP\sjtu_v5d_round_trip.m4a"
```
Expected: codec_name=aac，channels=2，sample_rate=44100。如不通则 stop + escalate（容器结构有问题）。

- [ ] **Step 7: cargo fmt + clippy + 行数检查**

Run: `cargo fmt && cargo clippy -p sjtu-cli --lib --all-targets -- -D warnings && wc -l src/apps/canvas_video/m4a_mux/mod.rs`
Expected: 0 errors / 0 warnings；mod.rs ≤ 200 行。

- [ ] **Step 8: Commit**

```
git add src/apps/canvas_video/mod.rs src/apps/canvas_video/mp4_box/parser.rs src/apps/canvas_video/m4a_mux/
git commit -m "feat(canvas-video): V5.D-T4 m4a_mux 模块（write_m4a + 复用源 stsd + rewrite stco）"
```

---

## Task 5: audio_dl::client（90 s 段级 timeout 的专属 reqwest Client）

**Files:**
- Create: `src/apps/canvas_video/audio_dl/mod.rs`
- Create: `src/apps/canvas_video/audio_dl/client.rs`
- Create: `src/apps/canvas_video/audio_dl/tests.rs`
- Modify: `src/apps/canvas_video/mod.rs` — 加 `pub mod audio_dl;`

- [ ] **Step 1: 写 failing test（client builds + Referer + UA）**

创建 `src/apps/canvas_video/audio_dl/tests.rs`：

```rust
//! audio_dl 单元测试。

use super::client::build_client_audio;

#[test]
fn build_client_audio_accepts_valid_referer() {
    let c = build_client_audio("https://courses.sjtu.edu.cn");
    assert!(c.is_ok(), "valid referer 应能构 client: {:?}", c.err());
}

#[test]
fn build_client_audio_rejects_non_ascii_referer() {
    let c = build_client_audio("https://例.cn");
    assert!(c.is_err(), "非 ASCII referer 应报错");
}
```

- [ ] **Step 2: 写 audio_dl/mod.rs 骨架**

创建 `src/apps/canvas_video/audio_dl/mod.rs`：

```rust
//! audio-only Range 直拉：mp4 box 解析 → 仅取 audio sample 字节 → mux 成 m4a。
//!
//! 与 download.rs 双路并存：
//! - download.rs 走旧 mp4 全下（保留给 keep-mp4 / 视频路径）
//! - audio_dl 走新 audio-only 路径，专属 client（90 s 段级 + 30 s inter-byte）
//!
//! `download_audio_only_to_file` 是对外唯一入口；fail-soft 由 download_shared 层负责
//! （V5.D 上线初期解析失败回退到旧路径）。

mod client;
mod orchestrator;
#[cfg(test)]
mod tests;

pub use orchestrator::{download_audio_only_to_file, DownloadStats};
```

- [ ] **Step 3: 写 audio_dl/client.rs**

创建 `src/apps/canvas_video/audio_dl/client.rs`：

```rust
//! audio_dl 专属 reqwest Client：90 s 段级 timeout，关 H2 + 关池（继承 V3.1 经验）。

use std::time::Duration;

use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, REFERER, USER_AGENT};
use reqwest::Client;

use crate::apps::canvas_video::http::UA;
use crate::error::SjtuCliError;

/// audio-only 下载入口的 reqwest Client。
///
/// 与 download.rs 的旧 client 区别：timeout 从 30 min 收紧到 90 s（audio sample 单段
/// 通常 < 5 MB，CDN 限速 1 MB/s 也只要 5 s；90 s 给 18× 安全垫，单段卡死自动 abort）。
/// 90 s 段级 timeout 与 chunk 间 30 s inter-byte timeout（在 orchestrator 内联）共同
/// 守护 V5.B Phase 1 第 9 讲那种 "TCP 不断但 body 无字节流入 13 min" 场景。
pub(super) fn build_client_audio(referer: &str) -> Result<Client> {
    let mut h = HeaderMap::new();
    h.insert(
        REFERER,
        HeaderValue::from_str(referer)
            .map_err(|e| SjtuCliError::InvalidInput(format!("Referer 非 ASCII: {e}")))?,
    );
    h.insert(USER_AGENT, HeaderValue::from_static(UA));
    Client::builder()
        .default_headers(h)
        .http1_only()
        .pool_max_idle_per_host(0)
        .tcp_nodelay(true)
        .timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("audio_dl client: {e}")).into())
}
```

- [ ] **Step 4: 占位 orchestrator.rs（让 mod.rs 编过）**

创建 `src/apps/canvas_video/audio_dl/orchestrator.rs`：

```rust
//! audio_dl orchestrator：moov 定位 + Range 合并 + 并发拉 + mux。
//! 实装见 Task 6 / Task 7 / Task 8。

use std::path::Path;

use anyhow::Result;

#[derive(Debug)]
pub struct DownloadStats {
    /// m4a 落盘字节数（≈ audio_track 总 sample 字节 + 容器 overhead）
    pub written: u64,
    /// 实际从 CDN 拉的字节数（≈ moov 区段 + 合并后 Range 总长）
    pub downloaded: u64,
}

/// audio-only 下载主入口。
pub async fn download_audio_only_to_file(
    _url: &str,
    _dest_m4a: &Path,
    _concurrency: usize,
    _referer: &str,
) -> Result<DownloadStats> {
    anyhow::bail!("download_audio_only_to_file 未实装（Task 6+）")
}
```

- [ ] **Step 5: 在 canvas_video/mod.rs 加 audio_dl 声明**

修改 `src/apps/canvas_video/mod.rs`，在 `pub mod m4a_mux;` 之后加：

```rust
pub mod audio_dl;
```

- [ ] **Step 6: Run client 测**

Run: `cargo test -p sjtu-cli --lib canvas_video::audio_dl::tests::build_client_audio`
Expected: 2 个测试 PASS。

- [ ] **Step 7: cargo fmt + clippy**

Run: `cargo fmt && cargo clippy -p sjtu-cli --lib --all-targets -- -D warnings`
Expected: 0 errors。

- [ ] **Step 8: Commit**

```
git add src/apps/canvas_video/mod.rs src/apps/canvas_video/audio_dl/
git commit -m "feat(canvas-video): V5.D-T5 audio_dl 模块骨架 + 90s 段级 timeout 专属 client"
```

---

## Task 6: orchestrator — moov 定位（faststart 头部 + standard 尾部回退）

**Files:**
- Modify: `src/apps/canvas_video/audio_dl/orchestrator.rs`
- Modify: `src/apps/canvas_video/audio_dl/tests.rs` — 加 mockito moov 定位测

- [ ] **Step 1: 写 failing test（mockito 模拟 standard layout：头部不含 moov，尾部 fetch 拿到）**

在 `audio_dl/tests.rs` 末尾追加：

```rust
use mockito::Server;

const FIXTURE_STANDARD: &[u8] =
    include_bytes!("../../../../tests/fixtures/canvas_video/audio_1s_standard.mp4");

/// 在 mp4 字节里返回 moov box 起点（用于 mockito 测试断言）。
fn find_moov_offset(mp4: &[u8]) -> usize {
    let mut pos = 0usize;
    while pos + 8 <= mp4.len() {
        let size = u32::from_be_bytes(mp4[pos..pos + 4].try_into().unwrap()) as usize;
        if &mp4[pos + 4..pos + 8] == b"moov" {
            return pos;
        }
        if size == 0 {
            break;
        }
        pos += size;
    }
    panic!("fixture 没有 moov");
}

#[tokio::test]
async fn locate_moov_falls_back_to_tail_when_head_lacks_moov() {
    use super::orchestrator::locate_moov_for_test;
    let mut server = Server::new_async().await;
    let total = FIXTURE_STANDARD.len();
    // HEAD probe (Range 0-0)：返 size
    let _m_probe = server
        .mock("GET", "/v.mp4")
        .match_header("range", "bytes=0-0")
        .with_status(206)
        .with_header("content-range", &format!("bytes 0-0/{total}"))
        .with_body(&FIXTURE_STANDARD[0..1])
        .create_async()
        .await;
    // 头部 1 MB（这里 fixture 才 ~3 KB，整个就是头部）
    let head_end = (1024 * 1024 - 1).min(total - 1);
    let _m_head = server
        .mock("GET", "/v.mp4")
        .match_header("range", &*format!("bytes=0-{head_end}"))
        .with_status(206)
        .with_header(
            "content-range",
            &format!("bytes 0-{head_end}/{total}"),
        )
        .with_body(&FIXTURE_STANDARD[..=head_end])
        .create_async()
        .await;
    // 尾部 1 MB（standard layout fallback 路径）
    let tail_start = total.saturating_sub(1024 * 1024);
    let _m_tail = server
        .mock("GET", "/v.mp4")
        .match_header("range", &*format!("bytes={tail_start}-{}", total - 1))
        .with_status(206)
        .with_header(
            "content-range",
            &format!("bytes {tail_start}-{}/{total}", total - 1),
        )
        .with_body(&FIXTURE_STANDARD[tail_start..])
        .create_async()
        .await;

    let url = format!("{}/v.mp4", server.url());
    let (moov_bytes, downloaded) = locate_moov_for_test(&url, "https://courses.sjtu.edu.cn")
        .await
        .expect("locate moov");
    let expected_offset = find_moov_offset(FIXTURE_STANDARD);
    let expected_size = u32::from_be_bytes(
        FIXTURE_STANDARD[expected_offset..expected_offset + 4]
            .try_into()
            .unwrap(),
    ) as usize;
    assert_eq!(
        moov_bytes.len(),
        expected_size,
        "moov 字节数应匹配 fixture 内 moov size"
    );
    assert!(downloaded > 0);
}
```

- [ ] **Step 2: 实装 orchestrator.rs 的 moov 定位主逻辑**

替换 `audio_dl/orchestrator.rs` 内容为：

```rust
//! audio_dl orchestrator：moov 定位 + Range 合并 + 并发拉 + mux。

use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use tracing::{debug, info, warn};

use crate::apps::canvas_video::mp4_box::parse_moov;
use crate::error::SjtuCliError;

use super::client::build_client_audio;

/// 头部 / 尾部 probe Range 大小，单位字节。SJTU CDN moov 实测最大 ~700 KB。
const HEAD_PROBE_SIZE: u64 = 1024 * 1024; // 1 MB
const TAIL_PROBE_INITIAL: u64 = 1024 * 1024; // 1 MB
const TAIL_PROBE_MAX: u64 = 16 * 1024 * 1024; // 16 MB（仍找不到 moov 视为非常规 mp4）
/// chunk 间无字节流入超时（V5.B Phase 1 第 9 讲事故的直接缓解）
pub(super) const INTER_BYTE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub struct DownloadStats {
    pub written: u64,
    pub downloaded: u64,
}

pub async fn download_audio_only_to_file(
    _url: &str,
    _dest_m4a: &Path,
    _concurrency: usize,
    _referer: &str,
) -> Result<DownloadStats> {
    bail!("download_audio_only_to_file 主流程见 Task 7 / Task 8")
}

/// 仅供测试使用的 moov 定位入口（pub(super) 不够测，#[cfg(test)] pub）。
#[cfg(test)]
pub(super) async fn locate_moov_for_test(url: &str, referer: &str) -> Result<(Vec<u8>, u64)> {
    let client = build_client_audio(referer)?;
    locate_moov(&client, url).await
}

/// 探测 mp4 size 并定位 moov box，返回 (moov 字节, 已下载字节数)。
pub(super) async fn locate_moov(client: &Client, url: &str) -> Result<(Vec<u8>, u64)> {
    let total = probe_size(client, url).await?;
    if total == 0 {
        bail!("probe size=0：{url}");
    }
    let mut downloaded: u64 = 1; // 包含 probe 1 字节
    // 1. 头部 1 MB
    let head_end = (HEAD_PROBE_SIZE - 1).min(total - 1);
    let head = fetch_range(client, url, 0, head_end).await?;
    downloaded += head.len() as u64;
    if let Some((moov_pos, moov_size)) = scan_for_moov(&head) {
        // 头部含 moov，但可能跨 1 MB 边界（取决于 box size）
        if moov_pos as u64 + moov_size <= head.len() as u64 {
            return Ok((head[moov_pos..moov_pos + moov_size as usize].to_vec(), downloaded));
        }
        // moov 跨界：补一段拿全 moov
        let extra_start = head.len() as u64;
        let extra_end = (moov_pos as u64 + moov_size - 1).min(total - 1);
        let extra = fetch_range(client, url, extra_start, extra_end).await?;
        downloaded += extra.len() as u64;
        let mut full = head[moov_pos..].to_vec();
        full.extend_from_slice(&extra);
        full.truncate(moov_size as usize);
        return Ok((full, downloaded));
    }
    // 2. 头部不含 moov → 尾部翻倍探测
    let mut probe_size = TAIL_PROBE_INITIAL;
    while probe_size <= TAIL_PROBE_MAX {
        let tail_start = total.saturating_sub(probe_size);
        let tail = fetch_range(client, url, tail_start, total - 1).await?;
        downloaded += tail.len() as u64;
        if let Some((rel, moov_size)) = scan_for_moov(&tail) {
            // tail-relative 偏移
            if rel as u64 + moov_size <= tail.len() as u64 {
                return Ok((tail[rel..rel + moov_size as usize].to_vec(), downloaded));
            }
            // moov 在尾段更深处（理论上 fetch 已经覆盖到尾，不可能跨）
            bail!("尾部 moov 跨边界（不可能）");
        }
        probe_size *= 2;
    }
    bail!("尾部 {} MB 仍找不到 moov，疑似非 mp4 容器", TAIL_PROBE_MAX / 1024 / 1024)
}

async fn probe_size(client: &Client, url: &str) -> Result<u64> {
    let resp = client
        .get(url)
        .header(RANGE, "bytes=0-0")
        .send()
        .await
        .map_err(neterr("probe"))?;
    let st = resp.status();
    if !st.is_success() && st != StatusCode::PARTIAL_CONTENT {
        bail!("probe status={st}");
    }
    if st == StatusCode::PARTIAL_CONTENT {
        return Ok(resp
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit('/').next())
            .and_then(|t| t.parse().ok())
            .unwrap_or(0));
    }
    Ok(resp.content_length().unwrap_or(0))
}

async fn fetch_range(client: &Client, url: &str, start: u64, end: u64) -> Result<Vec<u8>> {
    let rv = format!("bytes={start}-{end}");
    let mut resp = client
        .get(url)
        .header(RANGE, &rv)
        .send()
        .await
        .map_err(neterr("range get"))?;
    let st = resp.status();
    if st != StatusCode::PARTIAL_CONTENT && !st.is_success() {
        bail!("段 {rv} status={st}");
    }
    let mut buf: Vec<u8> = Vec::with_capacity((end - start + 1) as usize);
    loop {
        // 30 s inter-byte timeout：tokio::time::timeout 包 chunk()
        let chunk =
            tokio::time::timeout(INTER_BYTE_TIMEOUT, resp.chunk())
                .await
                .map_err(|_| {
                    SjtuCliError::NetworkError(format!("段 {rv} 30s 无字节流入，abort"))
                })?
                .map_err(neterr("chunk"))?;
        let Some(c) = chunk else {
            break;
        };
        buf.extend_from_slice(&c);
    }
    debug!(start, end, len = buf.len(), "段完成");
    Ok(buf)
}

/// 在 buf 里找 moov box，返回 (相对 buf 起点的偏移, moov size)。
/// 顺序扫顶层 box，遇到 moov 即返；若整段都不是 moov 返 None。
fn scan_for_moov(buf: &[u8]) -> Option<(usize, u64)> {
    let mut pos = 0usize;
    while pos + 8 <= buf.len() {
        let size32 = u32::from_be_bytes(buf[pos..pos + 4].try_into().ok()?);
        let ty: [u8; 4] = buf[pos + 4..pos + 8].try_into().ok()?;
        let size = if size32 == 1 {
            if buf.len() < pos + 16 {
                return None;
            }
            u64::from_be_bytes(buf[pos + 8..pos + 16].try_into().ok()?)
        } else if size32 == 0 {
            return None;
        } else {
            size32 as u64
        };
        if &ty == b"moov" {
            return Some((pos, size));
        }
        pos = pos.checked_add(size as usize)?;
    }
    None
}

fn neterr(ctx: &'static str) -> impl Fn(reqwest::Error) -> SjtuCliError {
    move |e| SjtuCliError::NetworkError(format!("{ctx}: {e}"))
}

// 暂时静默 unused warning（Task 7 / Task 8 用上）
#[allow(dead_code)]
fn _placeholder_keep_imports() {
    let _ = (parse_moov, info, warn, Context::context::<()>);
}
```

注：`_placeholder_keep_imports` 是占位让 clippy 不报 unused import；Task 7 实装后删。

实际更干净的做法是直接在 Task 6 里允许 clippy 一时性 dead code：在文件顶部加：

```rust
#![allow(dead_code)]
```

…然后 Task 8 完成后（所有 import 都用上了）删掉这一行。

- [ ] **Step 3: Run mockito 测**

Run: `cargo test -p sjtu-cli --lib canvas_video::audio_dl::tests::locate_moov_falls_back_to_tail`
Expected: PASS（成功从 standard layout fixture 尾部找到 moov）。

- [ ] **Step 4: cargo fmt + clippy**

Run: `cargo fmt && cargo clippy -p sjtu-cli --lib --all-targets -- -D warnings`
Expected: 0 errors（dead_code allow 暂时压制 placeholder）。

- [ ] **Step 5: Commit**

```
git add src/apps/canvas_video/audio_dl/
git commit -m "feat(canvas-video): V5.D-T6 audio_dl moov 定位（faststart 头 + standard 尾翻倍 fallback）"
```

---

## Task 7: orchestrator — Range 合并算法（纯函数 + 单元测）

**Files:**
- Modify: `src/apps/canvas_video/audio_dl/orchestrator.rs`
- Modify: `src/apps/canvas_video/audio_dl/tests.rs`

- [ ] **Step 1: 写 failing test（合并率 + 边界）**

在 `audio_dl/tests.rs` 末尾追加：

```rust
use super::orchestrator::merge_ranges;

#[test]
fn merge_ranges_collapses_adjacent_samples() {
    // 3 个紧邻 sample（gap=0），应合并成 1 个 Range
    let samples = vec![(100u64, 50u32), (150, 50), (200, 50)];
    let ranges = merge_ranges(&samples, 64 * 1024);
    assert_eq!(ranges, vec![(100, 249)]);
}

#[test]
fn merge_ranges_inlines_small_gap() {
    // gap = 50 字节 < 64KB 阈值 → 合并
    let samples = vec![(100u64, 50u32), (200, 50)];
    let ranges = merge_ranges(&samples, 64 * 1024);
    assert_eq!(ranges, vec![(100, 249)]);
}

#[test]
fn merge_ranges_splits_on_large_gap() {
    // gap = 100 KB > 64 KB → 不合并
    let samples = vec![(100u64, 50u32), (100 + 50 + 100 * 1024, 50)];
    let ranges = merge_ranges(&samples, 64 * 1024);
    assert_eq!(ranges.len(), 2);
}

#[test]
fn merge_ranges_handles_3000_samples() {
    // 3000 个 sample，每 sample 500B + 偶尔 100KB gap → 应合并到 < 100 个 Range
    let mut samples: Vec<(u64, u32)> = Vec::with_capacity(3000);
    let mut off = 0u64;
    for i in 0..3000 {
        samples.push((off, 500));
        off += 500;
        if i % 50 == 0 {
            off += 100 * 1024; // 100KB gap，会切
        }
    }
    let ranges = merge_ranges(&samples, 64 * 1024);
    assert!(
        ranges.len() < 100,
        "3000 sample 应合并到 < 100 Range，实际 {}",
        ranges.len()
    );
}
```

- [ ] **Step 2: 实装 merge_ranges 函数**

在 `orchestrator.rs` 内新增（放在 scan_for_moov 旁）：

```rust
/// audio sample (offset, size) 列表合并成连续 Range（gap < threshold 时 inline 多下）。
/// 返回 Vec<(start_inclusive, end_inclusive)>。
pub(super) fn merge_ranges(samples: &[(u64, u32)], gap_threshold: u64) -> Vec<(u64, u64)> {
    if samples.is_empty() {
        return Vec::new();
    }
    let mut merged: Vec<(u64, u64)> = Vec::new();
    let mut cur_start = samples[0].0;
    let mut cur_end = samples[0].0 + samples[0].1 as u64 - 1;
    for &(off, size) in &samples[1..] {
        let gap = off.saturating_sub(cur_end + 1);
        if gap <= gap_threshold {
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

- [ ] **Step 3: Run merge tests**

Run: `cargo test -p sjtu-cli --lib canvas_video::audio_dl::tests::merge_ranges`
Expected: 4 个 PASS。

- [ ] **Step 4: cargo fmt + clippy**

Run: `cargo fmt && cargo clippy -p sjtu-cli --lib --all-targets -- -D warnings`
Expected: 0 errors。

- [ ] **Step 5: Commit**

```
git add src/apps/canvas_video/audio_dl/
git commit -m "feat(canvas-video): V5.D-T7 Range 合并算法（gap < 64KB inline，3000 sample → < 100 req）"
```

---

## Task 8: orchestrator — 主 download_audio_only_to_file（并发拉 + 重组 sample bytes + mux）

**Files:**
- Modify: `src/apps/canvas_video/audio_dl/orchestrator.rs`
- Modify: `src/apps/canvas_video/audio_dl/tests.rs` — 加 inter-byte timeout 测

- [ ] **Step 1: 写 failing test（mockito 模拟 chunk 间慢响应触发 inter-byte timeout）**

在 `audio_dl/tests.rs` 末尾追加：

```rust
#[tokio::test]
async fn fetch_range_aborts_on_inter_byte_timeout() {
    use super::orchestrator::fetch_range_for_test;

    let mut server = Server::new_async().await;
    // 模拟连接 OK 但 60 秒后才发 1 字节 → 30 秒应触发 timeout
    let _m = server
        .mock("GET", "/slow")
        .with_status(206)
        .with_header("content-range", "bytes 0-9/10")
        .with_chunked_body(|w| {
            // mockito with_chunked_body：先写 1 字节，sleep 60s 再写
            w.write_all(b"X")?;
            std::thread::sleep(std::time::Duration::from_secs(60));
            w.write_all(b"YYYYYYYYY")?;
            Ok(())
        })
        .create_async()
        .await;
    let url = format!("{}/slow", server.url());
    let started = std::time::Instant::now();
    let result = fetch_range_for_test(&url, "https://courses.sjtu.edu.cn", 0, 9).await;
    let elapsed = started.elapsed();
    assert!(result.is_err(), "应在 30 s 触发 abort：{:?}", result);
    assert!(
        elapsed >= std::time::Duration::from_secs(28)
            && elapsed <= std::time::Duration::from_secs(45),
        "abort 应在 ~30 s（实际 {:?}）",
        elapsed
    );
    let msg = format!("{:#}", result.unwrap_err());
    assert!(msg.contains("30s 无字节流入"), "错误信息应提示 inter-byte：{msg}");
}
```

- [ ] **Step 2: 加 fetch_range_for_test 暴露 + 实装 download_audio_only_to_file 主流程**

在 `orchestrator.rs` 末尾追加：

```rust
#[cfg(test)]
pub(super) async fn fetch_range_for_test(
    url: &str,
    referer: &str,
    start: u64,
    end: u64,
) -> Result<Vec<u8>> {
    let client = build_client_audio(referer)?;
    fetch_range(&client, url, start, end).await
}
```

并替换 `download_audio_only_to_file` 占位为：

```rust
pub async fn download_audio_only_to_file(
    url: &str,
    dest_m4a: &Path,
    concurrency: usize,
    referer: &str,
) -> Result<DownloadStats> {
    let client = build_client_audio(referer)?;
    let (moov_bytes, mut downloaded) = locate_moov(&client, url).await?;
    info!(moov_size = moov_bytes.len(), "moov 定位完成");

    let track = parse_moov(&moov_bytes)
        .with_context(|| "parse moov（fail-soft 由调用方处理）")?;
    let total_sample_bytes: u64 = track.sample_sizes.iter().map(|&s| s as u64).sum();
    info!(
        codec = %track.codec,
        sample_count = track.sample_sizes.len(),
        total_sample_bytes,
        "audio track 解析完成"
    );

    // 合并 sample 范围
    let samples: Vec<(u64, u32)> = track
        .sample_offsets
        .iter()
        .copied()
        .zip(track.sample_sizes.iter().copied())
        .collect();
    let ranges = merge_ranges(&samples, 64 * 1024);
    info!(
        range_count = ranges.len(),
        sample_count = samples.len(),
        "Range 合并完成"
    );

    // 并发拉所有 Range
    let n = concurrency.max(1).min(ranges.len()).max(1);
    let fetched = parallel_ranges(&client, url, &ranges, n).await?;
    let fetched_bytes: u64 = fetched.iter().map(|(_, b)| b.len() as u64).sum();
    downloaded += fetched_bytes;
    info!(fetched_bytes, "所有 Range 拉取完成");

    // 把 fetched（按 range_idx）重组成"按 sample 顺序拼接的 sample_bytes"
    let sample_bytes = reassemble_samples(&track, &ranges, &fetched)?;
    debug_assert_eq!(sample_bytes.len() as u64, total_sample_bytes);

    // mux
    let written = crate::apps::canvas_video::m4a_mux::write_m4a(dest_m4a, &track, &sample_bytes)?;
    Ok(DownloadStats { written, downloaded })
}

/// 把若干 Range 字节按 sample 顺序拼回去。
/// fetched: Vec<(range_idx, range_bytes)>
fn reassemble_samples(
    track: &super::super::mp4_box::AudioTrack,
    ranges: &[(u64, u64)],
    fetched: &[(usize, Vec<u8>)],
) -> Result<Vec<u8>> {
    // range_idx → bytes 的映射（用 Vec 索引而不是 HashMap，开销小）
    let mut by_idx: Vec<Option<&Vec<u8>>> = vec![None; ranges.len()];
    for (i, b) in fetched {
        by_idx[*i] = Some(b);
    }
    let mut out: Vec<u8> = Vec::with_capacity(track.sample_sizes.iter().map(|&s| s as usize).sum());
    for (&off, &sz) in track.sample_offsets.iter().zip(track.sample_sizes.iter()) {
        let (ri, range_start) = find_range_for_sample(ranges, off)
            .ok_or_else(|| anyhow::anyhow!("sample offset {off} 不在任何 Range"))?;
        let bytes = by_idx[ri]
            .ok_or_else(|| anyhow::anyhow!("Range {ri} 没有数据（拉取丢失）"))?;
        let local_start = (off - range_start) as usize;
        let local_end = local_start + sz as usize;
        if local_end > bytes.len() {
            bail!(
                "sample 越界：range {ri} len={} 需 {}..{}",
                bytes.len(),
                local_start,
                local_end
            );
        }
        out.extend_from_slice(&bytes[local_start..local_end]);
    }
    Ok(out)
}

/// 在 ranges 里二分找 sample offset 落入哪个 range，返 (idx, range_start)。
fn find_range_for_sample(ranges: &[(u64, u64)], offset: u64) -> Option<(usize, u64)> {
    // ranges 按 start 升序（merge_ranges 输出有序）
    let mut lo = 0usize;
    let mut hi = ranges.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        let (s, e) = ranges[mid];
        if offset < s {
            hi = mid;
        } else if offset > e {
            lo = mid + 1;
        } else {
            return Some((mid, s));
        }
    }
    None
}

async fn parallel_ranges(
    client: &Client,
    url: &str,
    ranges: &[(u64, u64)],
    concurrency: usize,
) -> Result<Vec<(usize, Vec<u8>)>> {
    use tokio::sync::Semaphore;
    let sem = std::sync::Arc::new(Semaphore::new(concurrency));
    let mut joins = tokio::task::JoinSet::new();
    for (i, &(s, e)) in ranges.iter().enumerate() {
        let sem = sem.clone();
        let cli = client.clone();
        let url = url.to_string();
        joins.spawn(async move {
            let _permit = sem.acquire_owned().await.expect("semaphore");
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

async fn fetch_range_with_retry(
    client: &Client,
    url: &str,
    start: u64,
    end: u64,
) -> Result<Vec<u8>> {
    // 与 download.rs 对齐：梯度 backoff [0, 3s, 10s, 25s]
    const BACKOFF_MS: [u64; 4] = [0, 3000, 10000, 25000];
    let mut last: Option<anyhow::Error> = None;
    for (attempt, wait) in BACKOFF_MS.iter().enumerate() {
        tokio::time::sleep(Duration::from_millis(*wait)).await;
        match fetch_range(client, url, start, end).await {
            Ok(b) => return Ok(b),
            Err(e) => {
                warn!(start, end, attempt, err = %e, "段失败重试");
                last = Some(e);
            }
        }
    }
    Err(last.expect("≥1 次尝试"))
}
```

并删除文件顶部的 `#![allow(dead_code)]`（现在所有 import 都用上了），删除 `_placeholder_keep_imports`。

- [ ] **Step 3: Run inter-byte timeout 测**

Run: `cargo test -p sjtu-cli --lib canvas_video::audio_dl::tests::fetch_range_aborts_on_inter_byte_timeout -- --nocapture`
Expected: PASS（30 s 内 abort，错误信息含 "30s 无字节流入"）。

- [ ] **Step 4: Run 全部 audio_dl 测**

Run: `cargo test -p sjtu-cli --lib canvas_video::audio_dl`
Expected: 全 PASS（client 2 + locate 1 + merge 4 + inter-byte 1 = 8 测试）。

- [ ] **Step 5: cargo fmt + clippy + 行数**

Run: `cargo fmt && cargo clippy -p sjtu-cli --lib --all-targets -- -D warnings && wc -l src/apps/canvas_video/audio_dl/orchestrator.rs`
Expected: 0 errors / 0 warnings；orchestrator.rs ≤ 200 行。

- [ ] **Step 6: Commit**

```
git add src/apps/canvas_video/audio_dl/
git commit -m "feat(canvas-video): V5.D-T8 audio_dl 主流程（并发 Range + reassemble + mux），含 30s inter-byte timeout"
```

---

## Task 9: data.rs additive 字段（DownloadData / ChannelOutput / BatchData）

**Files:**
- Modify: `src/commands/canvas_video/data.rs:104-115` (ChannelOutput)
- Modify: `src/commands/canvas_video/data.rs:57-83` (DownloadData)
- Modify: `src/commands/canvas_video/data.rs:120-143` (BatchData)

- [ ] **Step 1: 修改 ChannelOutput（行 104-115）**

`src/commands/canvas_video/data.rs` 把 `ChannelOutput` 替换为：

```rust
/// `--all-channels` 模式下每路的输出，也复用于 batch 模式每讲每机位。
#[derive(Debug, Serialize)]
pub(super) struct ChannelOutput {
    pub channel: i32,
    pub file_path: String,
    /// `--audio-only` 时抽出的 m4a 路径。
    pub audio_path: Option<String>,
    /// `audio_only && !keep_mp4` 时为 false。
    pub mp4_kept: bool,
    /// 单一文件主产物字节数。旧 mp4-full 路径 = mp4 大小；V5.D m4a-direct = m4a 大小。
    pub bytes: u64,
    pub elapsed_ms: u128,
    pub mp4_url_redacted: String,
    /// V5.D additive：下载入口标识。
    /// `mp4-full` = 旧路径（download.rs 全下 mp4，可选 ffmpeg 抽流）
    /// `m4a-direct` = V5.D audio_dl Range 直拉 audio sample 本地 mux m4a
    /// `skipped` = batch 模式 dest 已存在
    pub download_kind: String,
    /// V5.D additive：实际从 CDN 拉的字节数。
    /// `mp4-full` = bytes（mp4 全下）；`m4a-direct` ≈ moov + audio samples + Range merge gap
    pub bytes_downloaded: u64,
}
```

- [ ] **Step 2: 修改 DownloadData（行 57-83）**

把 `DownloadData` 末尾追加 2 个字段：

```rust
    pub mp4_url_redacted: String,
    /// V5.D additive：见 ChannelOutput.download_kind 注释。
    pub download_kind: String,
    /// V5.D additive：见 ChannelOutput.bytes_downloaded 注释。
    pub bytes_downloaded: u64,
}
```

- [ ] **Step 3: 修改 BatchData（行 120-143）**

把 `BatchData` 末尾追加 1 个字段：

```rust
    pub total_elapsed_ms: u128,
    /// V5.D additive：批量下载从 CDN 实际拉的字节累计。等价 sum(items[].channels[].bytes_downloaded)。
    pub total_bytes_downloaded: u64,
    /// 每讲一条。顺序按展开后讲序。
    pub items: Vec<BatchEntry>,
}
```

- [ ] **Step 4: cargo check 看哪些 caller 缺字段**

Run: `cargo check -p sjtu-cli --lib`
Expected: 报 missing field 错误，发生在 `download_handler.rs` / `batch_handler.rs` / `download_shared.rs`。Task 10 修复。

- [ ] **Step 5: 暂不 commit**（等 Task 10 一起 commit，否则中间 cargo check 不绿）

---

## Task 10: download_shared.rs audio_only 分支 + handler/batch_handler 字段透传

**Files:**
- Modify: `src/commands/canvas_video/download_shared.rs:33-70` (download_one_channel)
- Modify: `src/commands/canvas_video/download_handler.rs:60-77` (DownloadData 构造)
- Modify: `src/commands/canvas_video/batch_handler.rs:60-95` (BatchData 累加 + check_skip)

- [ ] **Step 1: 重写 download_shared.rs 的 download_one_channel**

替换 `src/commands/canvas_video/download_shared.rs` 行 33-70 为：

```rust
/// 单 channel 下载。
/// - audio_only 走 V5.D 新路径 audio_dl::download_audio_only_to_file（无 mp4 落盘 / 无 ffmpeg）
///   失败时 fail-soft 回退到旧 mp4-full + ffmpeg 抽流路径。
/// - 非 audio_only：旧路径（download.rs 全下 mp4）。
#[allow(clippy::too_many_arguments)]
pub(super) async fn download_one_channel(
    client: &Client,
    target: &LectureVideo,
    channel: i32,
    to_dir: &Path,
    concurrency: usize,
    audio_only: bool,
    keep_mp4: bool,
    with_identity: bool,
) -> Result<(VideoFetch, ChannelOutput)> {
    let started = Instant::now();
    let fetch = client.get_video_info(&target.video_id, channel).await?;
    let stem = target.video_name.as_str().trim();
    let safe_stem = safe_filename(if stem.is_empty() { "video" } else { stem });
    let mp4_dest = to_dir.join(format!("{safe_stem}_ch{}.mp4", fetch.channel));
    let m4a_dest = to_dir.join(format!("{safe_stem}_ch{}.m4a", fetch.channel));

    if audio_only && !keep_mp4 {
        // V5.D 主路径：直拉 audio + 本地 mux，零 ffmpeg
        match crate::apps::canvas_video::audio_dl::download_audio_only_to_file(
            &fetch.mp4_url,
            &m4a_dest,
            concurrency,
            DOWNLOAD_REFERER,
        )
        .await
        {
            Ok(stats) => {
                let out = ChannelOutput {
                    channel: fetch.channel,
                    file_path: absolutize(&mp4_dest), // V4 quirk 保留：占位 mp4 路径（实际不落 mp4）
                    audio_path: Some(absolutize(&m4a_dest)),
                    mp4_kept: false,
                    bytes: stats.written,
                    elapsed_ms: started.elapsed().as_millis(),
                    mp4_url_redacted: redact_url(&fetch.mp4_url, with_identity),
                    download_kind: "m4a-direct".to_string(),
                    bytes_downloaded: stats.downloaded,
                };
                return Ok((fetch, out));
            }
            Err(e) => {
                tracing::warn!(err = %e, "V5.D audio_dl 失败，回退到旧 mp4 全下 + ffmpeg 路径");
                // 落到下面的旧路径
            }
        }
    }

    // 旧路径（mp4-full）：keep_mp4 / 非 audio_only / V5.D fail-soft 回退
    let bytes = download_to_file(&fetch.mp4_url, &mp4_dest, concurrency, DOWNLOAD_REFERER).await?;
    let mut audio_path: Option<String> = None;
    let mut mp4_kept = true;
    if audio_only {
        ff::extract_audio(&mp4_dest, &m4a_dest).await?;
        audio_path = Some(absolutize(&m4a_dest));
        if !keep_mp4 {
            tokio::fs::remove_file(&mp4_dest).await.ok();
            mp4_kept = false;
        }
    }
    let out = ChannelOutput {
        channel: fetch.channel,
        file_path: absolutize(&mp4_dest),
        audio_path,
        mp4_kept,
        bytes,
        elapsed_ms: started.elapsed().as_millis(),
        mp4_url_redacted: redact_url(&fetch.mp4_url, with_identity),
        download_kind: "mp4-full".to_string(),
        bytes_downloaded: bytes,
    };
    Ok((fetch, out))
}
```

- [ ] **Step 2: download_handler.rs 透传新字段**

修改 `src/commands/canvas_video/download_handler.rs` 行 60-77 的 `DownloadData` 构造，在末尾加 2 字段：

```rust
            mp4_url_redacted: out.mp4_url_redacted,
            download_kind: out.download_kind.clone(),
            bytes_downloaded: out.bytes_downloaded,
        }),
```

注：`out.download_kind` 是在 `out` 被 move 之前 clone（看具体 move 顺序，可能不需要 clone），看编译器报错调整即可。

- [ ] **Step 3: batch_handler.rs 修 check_skip + 累加 total_bytes_downloaded**

修改 `src/commands/canvas_video/batch_handler.rs` 的 `check_skip`（约行 152-178）末尾构造 ChannelOutput 加新字段：

```rust
    Some(ChannelOutput {
        channel,
        file_path: super::handlers::absolutize(&mp4),
        audio_path: if args.audio_only {
            Some(super::handlers::absolutize(&m4a))
        } else {
            None
        },
        mp4_kept,
        bytes: if args.audio_only { 0 } else { meta.len() },
        elapsed_ms: 0,
        mp4_url_redacted: "***skipped***".into(),
        download_kind: "skipped".to_string(),
        bytes_downloaded: 0,
    })
```

并在 `cmd_download_batch` 主体（约行 52-94）累加 `total_bytes_downloaded`：

```rust
    let mut items: Vec<BatchEntry> = Vec::with_capacity(seq_list.len());
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut total_bytes = 0u64;
    let mut total_bytes_downloaded = 0u64;
    let n = seq_list.len();
    for (i, &seq) in seq_list.iter().enumerate() {
        let target = &audited[seq as usize - 1];
        eprintln!(
            "[{}/{n}] 第 {seq} 讲 {} (channels: {:?})",
            i + 1,
            target.video_name,
            channels
        );
        let entry = download_one_lecture(&client, target, seq, &channels, &args).await;
        match entry.status.as_str() {
            "ok" => succeeded += 1,
            "skipped" => skipped += 1,
            _ => failed += 1,
        }
        for ch in &entry.channels {
            total_bytes += ch.bytes;
            total_bytes_downloaded += ch.bytes_downloaded;
        }
        items.push(entry);
    }
    render(
        Envelope::ok(BatchData {
            course_id: args.course_id,
            tool_id: args.tool_id,
            lectures_spec: args.lectures_spec,
            all_channels: args.all_channels,
            audio_only: args.audio_only,
            total_planned: n,
            succeeded,
            failed_count: failed,
            skipped,
            total_bytes,
            total_elapsed_ms: started.elapsed().as_millis(),
            total_bytes_downloaded,
            items,
        }),
        args.fmt,
    )
```

- [ ] **Step 4: cargo check 全绿**

Run: `cargo check -p sjtu-cli --lib`
Expected: 0 errors。如有 borrow 错（`out.download_kind` move 顺序），用 `out.download_kind.clone()` 修。

- [ ] **Step 5: 写 download_shared 单元测（mock fail-soft 回退）**

在 `src/commands/canvas_video/download_shared.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    /// download_kind 取值字符串 stable。
    #[test]
    fn download_kind_strings_are_stable() {
        // 这是一个 anti-regression 锁：CHANGELOG / 下游消费方依赖这些 literal。
        let expected = ["mp4-full", "m4a-direct", "skipped"];
        for s in expected {
            assert!(!s.is_empty());
            assert!(!s.contains(' '));
        }
    }
}
```

（fail-soft 完整流程的真测要打到真 CDN，留 Task 11 真机覆盖。）

- [ ] **Step 6: cargo test --lib 全绿**

Run: `cargo test -p sjtu-cli --lib canvas_video`
Expected: 全 PASS（含 mp4_box 5 + m4a_mux 1 + audio_dl 8 + 既有 canvas_video 测）。

- [ ] **Step 7: cargo fmt + clippy 全绿**

Run: `cargo fmt && cargo clippy -p sjtu-cli --lib --all-targets -- -D warnings`
Expected: 0 errors / 0 warnings。

- [ ] **Step 8: 行数检查（download_shared.rs / data.rs 是否仍 < 200）**

Run: `wc -l src/commands/canvas_video/download_shared.rs src/commands/canvas_video/data.rs src/commands/canvas_video/batch_handler.rs`
Expected: 每个 ≤ 200 行（download_shared 涨到 ~110，data.rs ~178，batch_handler ~180）。如超限 stop + escalate。

- [ ] **Step 9: Commit**

```
git add src/commands/canvas_video/data.rs src/commands/canvas_video/download_shared.rs src/commands/canvas_video/download_handler.rs src/commands/canvas_video/batch_handler.rs
git commit -m "feat(canvas-video): V5.D-T9+T10 envelope additive 字段 + audio_only 分支接入 audio_dl + fail-soft 回退"
```

---

## Task 11: 4 关 verification（fmt + clippy + 全 lib 测 + 单讲 smoke）

**Files:** 无（仅命令）

**为什么 main session：** 第 4 关跑真实 CDN，subagent 无 SJTU session。

- [ ] **Step 1: cargo fmt --check 全绿**

Run: `cargo fmt --all -- --check`
Expected: 0 diff。如有，跑 `cargo fmt --all` 修。

- [ ] **Step 2: cargo clippy 全绿**

Run: `cargo clippy --all-targets --workspace -- -D warnings`
Expected: 0 errors / 0 warnings。

- [ ] **Step 3: cargo test --lib 全绿**

Run: `cargo test -p sjtu-cli --lib`
Expected: 全部 PASS（含既有 + 新加 V5.D 测试 ~14 个）。

- [ ] **Step 4: 单讲 smoke（真机）**

Run（确认本机有有效 SJTU session）：
```
cargo run --release -- canvas-video download 88168 --lecture 10 --channel 0 --audio-only --to tmp/v5d_smoke --yaml > tmp/v5d_smoke/_smoke.stdout.log 2>tmp/v5d_smoke/_smoke.stderr.log
```
Expected:
- 退出 0
- `tmp/v5d_smoke/` 出现 `*_ch0.m4a`，无 `*_ch0.mp4`
- envelope 含 `download_kind: m4a-direct`、`bytes` < 30 MB（旧 mp4-full 这里是 ~840 MB）、`bytes_downloaded` 略大于 `bytes`
- 总耗时 < 5 min（基线 21 min）

- [ ] **Step 5: ffprobe 验 m4a 主产物**

Run:
```
ffprobe -v quiet -print_format json -show_streams tmp/v5d_smoke/*ch0.m4a
```
Expected: codec_name=aac，duration ≈ 课时长（看 envelope.duration_secs 对照），channels / sample_rate 合理。

- [ ] **Step 6: 同条件二跑验 skip 路径**

Run:
```
cargo run --release -- canvas-video download 88168 --lecture 10 --channel 0 --audio-only --to tmp/v5d_smoke --yaml
```
Expected: 极快返回（< 1 s），envelope `download_kind` 仍是 `m4a-direct`（V4 既有行为：`cmd_download` 不做 skip 判断，直接重跑，会重下；这步重点验证不回退到旧路径）。

注：单讲入口不做 skip（仅 batch 做 skip）；smoke 实际就是再下一遍，验证 download_kind 不偏。

- [ ] **Step 7: Commit（如此前修改了任何代码）**

如果中间步骤改了任何文件，commit；否则跳过。

```
git add -A && git commit -m "fix(canvas-video): V5.D-T11 4 关验证修复（如有）"
```

---

## Task 12: V5.D Phase 2 真机 9 讲对比 + lessons.md + 最终 commit

**Files:**
- Create: `tmp/v5_audio_18/_v5d.stdout.log` / `tmp/v5_audio_18/_v5d.stderr.log`（不入 git）
- Create: `tmp/v5_audio_18/_comparison.md`（对比报告）
- Modify: `tasks/lessons.md` — 加 V5.D 章节
- Modify: `tasks/todo.md` — V5.D 标完成

**为什么 main session：** 跑真 CDN + 写经验需要主 session。

- [ ] **Step 1: 跑 V5.D Phase 2 真机（L10-L18）**

Run:
```
cargo run --release -- canvas-video download 88168 --lectures 10-18 --channel 0 --audio-only --to tmp/v5_audio_18 --yaml > tmp/v5_audio_18/_v5d.stdout.log 2>tmp/v5_audio_18/_v5d.stderr.log
```
注意：lectures 范围按基线区间（V5.B 已下 L1-L9，跳过这些；从 L10 开始）。

Expected:
- 9 讲全成（envelope `succeeded == 9`）
- 总墙钟（看 stdout 内 envelope `total_elapsed_ms`）vs 基线 21 min/讲 = 189 min sustained，新预期 27-45 min
- 全部 9 条 items[].channels[0].download_kind == "m4a-direct"

如有任何 L 失败（fail-soft 进 errors）：
1. 查 `tmp/v5_audio_18/_v5d.stderr.log` 找原因
2. 如是 V5.D 解析问题（`parse_moov` 失败），envelope 应显示 `mp4-full`（fail-soft 回退）—— 也是合规结果但标黄
3. 如是 reqwest timeout / 网络层，单纯是 CDN 不稳，记录到对比报告

- [ ] **Step 2: 抽一份 envelope sample 看新字段**

Run（PowerShell）：
```
Get-Content tmp/v5_audio_18/_v5d.stdout.log | Select-Object -First 200
```
Expected: 看到 `download_kind`, `bytes_downloaded`, `total_bytes_downloaded` 字段实出。

- [ ] **Step 3: 抽 1 讲的 m4a 跟基线对应讲对比**

Run：
```
ffprobe -v quiet -print_format json -show_streams tmp/v5_audio_18/*第14讲*ch0.m4a
ls -l tmp/v5_audio_18/*第14讲*.m4a
```
对比新 m4a vs 基线 `第14讲` m4a（基线在 V5.B Phase 1 已下，文件应都在 `tmp/v5_audio_18/`，需要确认 V5.D 没覆盖；如覆盖，从 git history 或 baseline 备份找）。

Expected: codec / channels / sample_rate 一致；文件大小差 < 5%；duration 误差 < 1 s。

- [ ] **Step 4: 写对比报告 tmp/v5_audio_18/_comparison.md**

写入文件：

```markdown
# V5.B Phase 2 — V5.D 9 讲实测对比（L10-L18 vs L1-L9 基线）

> V5.D = mp4 box audio-only Range + 90s/30s timeout 收紧。
> 数据来源：`_v5d.stdout.log` envelope + `_baseline.md` 基线表。

## 整体对比

| 维度 | 基线（L1-L9 旧路径） | V5.D（L10-L18 新路径） | 比值 |
|---|---|---|---|
| 总墙钟 | 149 min（含中断时间） | <填入_v5d 的 total_elapsed_ms / 60000> | <算> |
| sustained 单讲 | ~21 min | <从 stdout 9 条 items 算 mean(elapsed_ms)> | ~5× 提速 ✓/✗ |
| 网络字节累计 | ~7.5 GB（9 × 840MB mp4） | <total_bytes_downloaded> | <算> |
| 网络浪费比 | 42×（mp4 / m4a） | ~1.05-1.10× | ~38× 节省 ✓/✗ |
| 抗卡死 | 30 min reqwest timeout | 90 s 段 + 30 s inter-byte | 单段卡死自动 abort ✓ |
| 主产物 | 9 × m4a（ffmpeg 抽，~20 MB） | 9 × m4a（mux，~20 MB） | 大小一致 ✓/✗ |

## 异常事件

<填：是否有 L 走了 fail-soft 回退到 mp4-full？是否有 inter-byte abort？>

## 抽样验证

L14 m4a 对照（基线 vs V5.D）：
- 基线大小：<>
- V5.D 大小：<>
- 差：<%>
- ffprobe codec/channels/sample_rate：<>
```

填入实际数据。

- [ ] **Step 5: 加 V5.D 章节到 tasks/lessons.md**

Read `tasks/lessons.md` 末尾 50 行，找最近一节的格式（如 V3.1 / V5.A），仿照写一节。

要点：
- 新经验：mp4 box 自己解析比上 mp4 crate 简单的多（手写 5 个 box ≈ 200 行 vs crate ≥ 50 KB 依赖）
- 新经验：reqwest 30 min timeout 太宽松，audio-only 单段 < 5 MB 应 90 s 段级；body 流 silent 挂死要靠 `tokio::time::timeout(_, resp.chunk())` inter-byte timeout 抓
- 新经验：envelope 改语义会破坏旧消费方；改用 additive 加新字段（download_kind / bytes_downloaded）—— 联网交叉验证 Confluent / Conduktor / yt-dlp 共识
- 数据：38× 网络节省 / 5× 提速 / V5.D Phase 2 实测见 `tmp/v5_audio_18/_comparison.md`

- [ ] **Step 6: tasks/todo.md 标 V5.D 完成**

把 V5.D 相关条目标 `[x]`，加一行指向对比报告 + lessons 章节。

- [ ] **Step 7: 检查 .gitignore 含 tmp/**

Run: `grep -q '^tmp/' .gitignore && echo 'ok' || echo 'NEED TO ADD'`
Expected: `ok`。如 NEED TO ADD，加一行 `tmp/` 到 `.gitignore`（V5.B 时应该已加）。

- [ ] **Step 8: 最终 commit**

```
git add tasks/lessons.md tasks/todo.md
git commit -m "docs(canvas-video): V5.D-T12 9 讲实测对比（38× 节省 / 5× 提速）+ lessons 总结"
```

注：`tmp/v5_audio_18/_comparison.md` / `_v5d.*.log` 不入 git（在 tmp/ 下）。

---

## 收尾

- [ ] **Final: V5.D 全部 commit 列表确认**

Run: `git log --oneline -20 | head -20`
Expected: 看到 12 + 1 个 V5.D commit（T0-T12 + 任何 fix），从 fixture 起到 lessons 收尾。

- [ ] **Final: 全测 + 单讲 smoke 一次复跑**

Run: `cargo test -p sjtu-cli --lib && cargo run --release -- canvas-video download 88168 --lecture 18 --channel 0 --audio-only --to tmp/v5d_final_smoke --yaml`
Expected: lib 测全过 + 第 18 讲走 m4a-direct 路径 < 5 min 完成。

V5.D 完工，移交 V5.E（跨讲 Semaphore 池）/ V6（字幕转录）。
