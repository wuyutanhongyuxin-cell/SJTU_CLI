# V5.F 设计：撤 audio-only 整路，回归 mp4-full + ffmpeg

> 日期：2026-05-12
> Supersedes：
> - `2026-05-10-v5d-audio-only-range-design.md`（V5.D — audio-only Range merge，6.5 min/讲 working baseline）
> - `2026-05-11-v5e-b-h2-gap-sweep-design.md`（V5.E-B 原版）
> - `2026-05-11-v5e-b-plus-multi-h2-p85-design.md`（V5.E-B+ — 4-Client H2 + Dynamic P85，30.5 min/讲 反向退化）
> 决策证据：
> - `docs/superpowers/research/2026-05-11-v5e-b-cross-validation.md`（V5.E-B+ 前 4-agent 验证）
> - `docs/superpowers/research/2026-05-12-v5e-b-plus-regression-postmortem.md`（V5.E-B+ 反向退化 4-agent 验证 —— 待写，落 lessons）
> - 30s 真机 audio-endpoint probe（11 个 URL variant 全 404）
> - SJTU mp4 hexdump（`ftyp` 直接接 `mdat`，moov-end 格式，stdin pipe 不可行）

---

## 一句话

撤回 V5.D + V5.E-B+ 整条 audio-only 优化路径（删 `audio_dl` / `m4a_mux` / `mp4_box` 三个模块，约 1500 行代码 + ~30 单元测），让 `audio_only && !keep_mp4` 走 V5.A 旧 mp4-full + ffmpeg 路径，接受 916 MB/讲 网络代价以换取 2-2.5 min/讲 的稳定 wall-clock。

## 决策背景（为什么不再继续 audio-only）

| 已穷尽的 audio-only 优化方向 | 真机结果 |
|---|---|
| V5.A mp4-full + ffmpeg（baseline） | ✅ 916 MB / **~2 min/讲** |
| V5.D audio_dl + H1.1 × 8 + fixed 64KB gap_threshold | ⚠ 705 MB / 6.5 min/讲（已偏离 3 min 目标）|
| V5.E-B+ 4-Client H2 池 + Dynamic P85 gap | ❌ 575 MB / **30.5 min/讲**（4.7× 退化）|
| 11 variant audio-only endpoint probe | ❌ 全 404（CDN 不提供）|
| ffmpeg stdin pipe（B.1 微优化） | ❌ moov-end 格式不可流式（hexdump 证实）|
| HTTP/3 / DASH / HLS / aria2 sparse Range | ❌ 4 agent 上轮 web research 全部否决 |

**结论**：在 SJTU CDN 的 moov-end mp4 + 无 audio-only endpoint 双约束下，audio-only Range 优化的理论上限远高于"整下并发"。继续追 < 3 min/讲 目标是无效消耗。

## Goal

| 指标 | V5.A baseline | V5.D | V5.E-B+ | **V5.F 目标** |
|---|---|---|---|---|
| 单讲 elapsed | ~2 min | 6.5 min | 30.5 min | **≤ 2.5 min** |
| 9 讲 batch | ~18 min | ~60 min | > 4 h | **≤ 25 min** |
| 单讲网络 | 916 MB | 705 MB | 575 MB | **916 MB**（接受）|
| 磁盘 IO/讲 | 写 916 MB mp4 → 抽 22 MB m4a → 删 mp4 | 写 22 MB m4a | 写 22 MB m4a | **同 V5.A** |
| 外部依赖 | reqwest + ffmpeg | 仅 reqwest | 仅 reqwest | **reqwest + ffmpeg**（必装）|
| 代码复杂度 | 简单 | 高（3 模块）| 极高 | **同 V5.A**（删 3 模块）|
| 维护性 | 高 | 中 | 低 | **高** |

## 改动清单（精确到文件 / 行）

### 改动 1：删整个 `src/apps/canvas_video/audio_dl/` 目录

| 文件 | 行数（估）|
|---|---|
| `mod.rs` | ~30 |
| `client.rs` | 117（V5.E-B+ 改造后）|
| `fetch.rs` | ~180 |
| `locate.rs` | ~120 |
| `orchestrator.rs` | ~180 |
| `ranges.rs` | ~150（V5.E-B+ 加 compute_p85_gap）|
| `tests.rs` | ~200 |
| `test_helpers.rs` | ~80 |

### 改动 2：删整个 `src/apps/canvas_video/m4a_mux/` 目录

| 文件 | 行数（估）|
|---|---|
| `mod.rs` | ~20 |
| `build_moov.rs` | ~180 |
| `tests.rs` | ~120 |

### 改动 3：删整个 `src/apps/canvas_video/mp4_box/` 目录

| 文件 | 行数（估）|
|---|---|
| `mod.rs` | ~30 |
| `boxes.rs` | ~150 |
| `parser.rs` | ~180 |
| `stbl.rs` | ~170 |
| `trak.rs` | ~120 |
| `tests.rs` | ~200 |

### 改动 4：`src/apps/canvas_video/mod.rs`

```rust
// 删除：
pub mod audio_dl;       // line 14
pub mod m4a_mux;        // line 21
pub mod mp4_box;        // line 24
```

保留 `pub mod download;` / `pub mod ffmpeg;`（V5.A 路径依赖）。

### 改动 5：`src/commands/canvas_video/download_shared.rs`

**删 line 54-88**（V5.D audio_dl 调用块 + 回退分支）。`audio_only && !keep_mp4` 直接 fallthrough 进 line 91+ 的旧 mp4-full + ffmpeg 路径。

**改 line 121**：
```rust
// 旧
let expected = ["mp4-full", "m4a-direct", "skipped"];
// 新
let expected = ["mp4-full", "skipped"];
```

**改 line 32-35 doc comment**：删 V5.D 部分，改为简单一行说明 audio_only / keep_mp4 组合逻辑。

### 改动 6：`src/commands/canvas_video/data.rs` 字段注释清理

- `ChannelOutput.bytes` 注释（line 116-117）：删 "V5.D m4a-direct = m4a 大小" 部分
- `ChannelOutput.download_kind` 注释（line 120-124）：删 `m4a-direct` 描述，只留 `mp4-full` / `skipped`
- `ChannelOutput.bytes_downloaded` 注释（line 125-127）：删 m4a-direct 公式，简化为 `= bytes`（mp4-full 路径下两者恒等）
- `DownloadData.download_kind` / `bytes_downloaded` 注释（line 83-86）：同上简化
- `BatchData.total_bytes_downloaded` 注释（line 154）：删 V5.D 字眼

**字段本身不删**（envelope additive 契约：删字段是 breaking change，下游 AI Agent 可能依赖）。

### 改动 7：Cargo.toml feature 收缩（可选）

reqwest 之前为 V5.E-B+ 加了 `"http2"` feature。V5.F 走 H1.1 + connection pool，可以撤回，但不撤也无害（仅 ~50 KB binary size 影响）。**决策：保留 `http2` feature**，理由：未来若 SJTU CDN 升级 H2 兼容 ffmpeg 等场景仍可受益，且撤回需 commit 单独的 Cargo.toml 改动增加 review 面。

## 测试矩阵

| 测试 | 工具 | 通过条件 |
|---|---|---|
| `cargo build --release` | bash | 无 unresolved import / dead code 警告 |
| `cargo test --lib` | bash | 现有测中删去 audio_dl/m4a_mux/mp4_box 模块测后，剩余测全绿（预期 124 - 30 ≈ 94）|
| `cargo clippy --all-targets -- -D warnings` | bash | 零 warning |
| `cargo fmt --check` | bash | 零 diff |
| `download_kind_strings_are_stable` 单元测 | bash | `["mp4-full", "skipped"]` 通过 |
| 真机 T4：L10 单讲 smoke | bash time | elapsed ≤ 2.5 min，download_kind="mp4-full"，audio_path.m4a > 5 MB |
| 真机 T5：9 讲 batch | bash time | total ≤ 25 min，9/9 mp4-full，所有 m4a 落盘 |

## 不在范围（防 scope creep）

- ❌ ffmpeg stdin pipe 流式抽（B.1）—— SJTU mp4 moov-end，hexdump 已否
- ❌ 自实现 qt-faststart 重排 moov —— 需先下完整 mp4，等价 V5.A
- ❌ 任何 audio-only Range 优化复活 —— V5.D + V5.E-B+ 已 5h 沉没
- ❌ Cargo.toml reqwest feature 收缩
- ❌ envelope `download_kind` / `bytes_downloaded` 字段删除（breaking change）
- ❌ batch 模式并发改造（仍顺序下，单讲 H1.1 × 8 conn 已饱和 SJTU 带宽）
- ❌ V5.E-B+ commits revert（直接 hand-edit，不破坏 git history 的 audit trail）

## Fail-soft 行为

V5.F **没有 fallback**：单 mp4-full + ffmpeg 路径。
- 下载失败 → 上抛 `SjtuCliError::NetworkError`，envelope `error.message`
- ffmpeg 缺失 → `prep()` 早爆（line 22-25）
- mp4 抽流失败（损坏） → 上抛错误，**不**保留 mp4（避免 `tmp/` 堆积），让用户重试

## 完成判定（关闭 task #42）

✅ **关闭**（全部满足）：
- 17 文件已删，mod.rs / download_shared.rs / data.rs 已改
- cargo test + clippy + fmt 全绿
- 真机 L10 单讲 ≤ 2.5 min，download_kind="mp4-full"
- 真机 9 讲 batch ≤ 25 min，9/9 mp4-full
- 所有 m4a 落盘 > 5 MB
- tasks/lessons.md + CLAUDE.md 已同步

⚠ **部分达成**（仍关 + 记 lessons）：
- 单讲 2.5-3 min（SJTU 带宽波动）
- 9 讲 batch 25-30 min

❌ **不关闭**：
- 真机 batch > 30 min（已比 V5.A baseline 慢 → 怀疑 reqwest 8 conn split 退化，需独立调研，不在 V5.F 范围）

## 风险 & 缓解

1. **V5.A 旧路径已 5 个月没真机跑**（V3/V4 mp4 工件留在 `tmp/`，可能与现 CDN 协议已偏）
   - 缓解：T4 单讲 smoke 优先，失败立即 rollback hand-edit（不 commit 直到 T5 通过）

2. **~~reqwest 8 conn × H1.1 split 在新 ALPN 环境下退化~~**（已排除）
   - `src/apps/canvas_video/download.rs` 已 hardcode `Client::builder().http1_only().pool_max_idle_per_host(0)`（line 48-50），即使 Cargo.toml `http2` feature 启用，本路径强制 H1.1 × 8 独立 TCP
   - V5.A 行为完全保留，无需 `SJTU_FORCE_HTTP1` env

3. **删 audio_dl 后 `audio_only && !keep_mp4` 临时落盘 916 MB mp4** —— 用户磁盘可能不足
   - 缓解：no-op（916 MB 临时占用 + 抽完即删，与 V5.A 同行为；用户的 `tmp/v3` 历史工件已证可承受）

4. **下游 AI Agent 期待 `download_kind="m4a-direct"`** —— envelope 取值集合收窄
   - 缓解：SCHEMA.md / SKILL.md 同步更新 `download_kind` 枚举值；前向兼容（消费方应已按 stable 字符串集合处理 unknown）

## Lessons 待记录（task #65 执行时落盘到 `tasks/lessons.md`）

1. **CDN audio-only endpoint 探测先于优化设计**：早 1 周做 11 variant probe 能阻止 V5.D + V5.E-B+ 5h 工时
2. **HTTP/2 单 Client buffer bug（reqwest #1276）真机优先于理论**：4-Client H2 池纸面优雅，CDN 实际限流 + Tengine 128 stream 限制 + SETTINGS_INITIAL_WINDOW_SIZE 65535 三层叠加导致 4.7× 退化
3. **mp4 moov 位置 hexdump 范式**：4 字节 ftyp size + 4 字节 ftyp type + 接下来 box 类型 = `moov` (faststart) / `mdat` (non-faststart)，决定 stdin pipe 可行性
4. **fail-soft 不应掩盖性能退化**：V5.D fail-soft → V5.A 让真机看不出 audio_dl 失败，应加显式 warn 计数和 envelope 统计

## 仓库决策记录

- V5.D / V5.E-B+ commits 不 revert，保留 git history audit trail；本 spec + lessons 是其墓志铭
- audio_dl/m4a_mux/mp4_box 三模块代码不归档（删干净，未来若重启需重写。归档复活率 < 5%，徒增 noise）
