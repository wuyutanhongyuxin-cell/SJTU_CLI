# V5.F 实装计划：撤 audio-only 整路，回归 mp4-full + ffmpeg

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development（T1-T3）；T4-T5 main session 亲跑（subagent 无 ffmpeg / 无 SJTU session）。Steps 用 checkbox (`- [ ]`) 跟踪。

**Goal**：删 V5.D + V5.E-B+ 引入的 `audio_dl` / `m4a_mux` / `mp4_box` 三个模块（17 文件 / ~1500 行），让 `audio_only && !keep_mp4` 走 V5.A 旧 mp4-full + ffmpeg 路径。单讲 ≤ 2.5 min，9 讲 batch ≤ 25 min，关 task #42。

**Architecture**：单一下载路径（`download::download_to_file` H1.1 × 8 → 可选 `ffmpeg::extract_audio` → 删 mp4）。Envelope `download_kind` 字段保留但取值收窄成 `{mp4-full, skipped}`。

**Tech Stack**：reqwest（已含）/ tokio / ffmpeg CLI（用户必装）。

**Spec**：`docs/superpowers/specs/2026-05-12-v5f-mp4-full-plus-ffmpeg-design.md`

**任务总数**：5 个（T1-T3 subagent / T4-T5 main session）

---

## 工作目录与分支

- 在 main repo 根目录 `E:\claude_ask\sjtu_CLI\sjtu-cli` 操作
- **不**新开 worktree（V5.E-B+ smoke 工件 `tmp/v5e_b_plus/` 留作 lessons 参考）
- 分支：当前 main（V5.E-B+ commits c0935e3 / 8a243ac / b8f55a1 / 29d05d1 已在 history）
- 每 task 一 commit，commit msg 前缀 `refactor(canvas_video):` / `feat(canvas_video):`

---

## Task 1：简化 `download_shared.rs` + 清理 `data.rs` 字段注释

**Files**：
- Modify: `src/commands/canvas_video/download_shared.rs:10-127`
- Modify: `src/commands/canvas_video/data.rs:83-127, 154`

**Steps**：

- [ ] **Step 1：改 `download_shared.rs` 删 V5.D 调用块**

打开 `src/commands/canvas_video/download_shared.rs`，做 3 处改动：

a. line 10-13 imports：保持不动（`download_to_file` / `ffmpeg as ff` / `Client` / `LectureVideo` / `VideoFetch` / `SjtuCliError` 旧路径都还需要）

b. line 32-35 doc comment 改为：
```rust
/// 单 channel 下载。
/// V5.F：单一路径 mp4-full → 可选 ffmpeg 抽 audio → audio_only && !keep_mp4 时删 mp4。
/// V5.D + V5.E-B+ audio-only 优化整路已撤回（见 docs/superpowers/specs/2026-05-12-v5f-*.md）。
```

c. **删 line 54-88 整段**（V5.D `if audio_only && !keep_mp4 { audio_dl::... }` 块），删后 line 91（`// 旧路径（mp4-full）`）直接接在 line 53（`let m4a_dest = ...`）后面。

d. line 90 的注释 `// 旧路径（mp4-full）：keep_mp4 / 非 audio_only / V5.D fail-soft 回退` 改为 `// 单一路径（mp4-full）：可选 ffmpeg 抽 audio`

e. line 121 tests 字符串数组：
```rust
// 旧
let expected = ["mp4-full", "m4a-direct", "skipped"];
// 新
let expected = ["mp4-full", "skipped"];
```

- [ ] **Step 2：改 `data.rs` 字段注释**

a. line 83-86（`DownloadData.download_kind` / `bytes_downloaded`）：
```rust
// 旧
/// V5.D additive：见 ChannelOutput.download_kind 注释。
pub download_kind: String,
/// V5.D additive：见 ChannelOutput.bytes_downloaded 注释。
pub bytes_downloaded: u64,
// 新
/// 下载入口标识。取值见 ChannelOutput.download_kind。
pub download_kind: String,
/// 实际从 CDN 拉的字节数。mp4-full 路径下等于 `bytes`。
pub bytes_downloaded: u64,
```

b. line 116-117（`ChannelOutput.bytes` 注释）：
```rust
// 旧
/// 单一文件主产物字节数。旧 mp4-full 路径 = mp4 大小；V5.D m4a-direct = m4a 大小。
pub bytes: u64,
// 新
/// 单一文件主产物字节数（mp4 大小，或 audio_only && !keep_mp4 模式下 mp4 删前的原始大小）。
pub bytes: u64,
```

c. line 120-127（`ChannelOutput.download_kind` / `bytes_downloaded` 注释）：
```rust
// 旧
/// V5.D additive：下载入口标识。
/// `mp4-full` = 旧路径（download.rs 全下 mp4，可选 ffmpeg 抽流）
/// `m4a-direct` = V5.D audio_dl Range 直拉 audio sample 本地 mux m4a
/// `skipped` = batch 模式 dest 已存在
pub download_kind: String,
/// V5.D additive：实际从 CDN 拉的字节数。
/// `mp4-full` = bytes（mp4 全下）；`m4a-direct` ≈ moov + audio samples + Range merge gap
pub bytes_downloaded: u64,
// 新
/// 下载入口标识。
/// `mp4-full` = download.rs 全下 mp4，可选 ffmpeg 抽流
/// `skipped` = batch 模式 dest 已存在
pub download_kind: String,
/// 实际从 CDN 拉的字节数。mp4-full 路径下等于 `bytes`。
pub bytes_downloaded: u64,
```

d. line 154（`BatchData.total_bytes_downloaded` 注释）：
```rust
// 旧
/// V5.D additive：批量下载从 CDN 实际拉的字节累计。等价 sum(items[].channels[].bytes_downloaded)。
pub total_bytes_downloaded: u64,
// 新
/// 批量下载从 CDN 实际拉的字节累计。等价 sum(items[].channels[].bytes_downloaded)。
pub total_bytes_downloaded: u64,
```

- [ ] **Step 3：cargo check 验证编译（仍会因为 mod.rs 未删 mod 而 dead_code 警告 audio_dl，T2 解决）**

```powershell
cargo check 2>&1 | Select-String -Pattern 'error\['
```

预期：**零 error**。dead_code warning 不报错（Cargo.toml 应有 `lints.rust.dead_code = "warn"` 或默认）。

- [ ] **Step 4：commit**

```bash
git add src/commands/canvas_video/download_shared.rs src/commands/canvas_video/data.rs
git commit -m "refactor(canvas_video): 简化 download_shared 撤 V5.D audio_dl 调用 + 清理 data.rs 注释"
```

**Exit criteria**：cargo check 0 error，commit 落地。

---

## Task 2：删 `mod.rs` 3 个 `pub mod` + git rm -r 3 个目录

**Files**：
- Modify: `src/apps/canvas_video/mod.rs:14, 21, 24`
- Delete: `src/apps/canvas_video/audio_dl/` (8 files)
- Delete: `src/apps/canvas_video/m4a_mux/` (3 files)
- Delete: `src/apps/canvas_video/mp4_box/` (6 files)

**Steps**：

- [ ] **Step 1：改 `src/apps/canvas_video/mod.rs`**

删除 line 14 / 21 / 24 三个 `pub mod` 声明。保留 line 18（`pub mod download;`）/ line 19（`pub mod ffmpeg;`）等 V5.A 路径依赖。

改完后 `mod.rs` 应该这样：
```rust
//! 课堂视频（v.sjtu.edu.cn / 交我学）客户端。
//! ...（doc 不动）

mod api;
mod api_form;
pub mod auth;
mod auth_chrome;
pub(crate) mod cache;
pub mod download;
pub mod ffmpeg;
mod http;
mod models;
mod models_video;
#[cfg(test)]
mod tests_cache;
#[cfg(test)]
mod tests_parse;
mod throttle;

pub use api::{Client, VideoFetch};
pub use models::{Bootstrap, LectureVideo};
```

- [ ] **Step 2：git rm -r 3 个目录**

```bash
git rm -r src/apps/canvas_video/audio_dl/
git rm -r src/apps/canvas_video/m4a_mux/
git rm -r src/apps/canvas_video/mp4_box/
```

预期：git stage 17 文件 deletion。

- [ ] **Step 3：cargo check 验证编译**

```powershell
cargo check 2>&1 | Select-String -Pattern 'error\['
```

预期：**零 error**。如果有 error，大概率是：
- 某测试文件仍 `use crate::apps::canvas_video::audio_dl::...` → 改测试 file 删 use
- `src/commands/canvas_video/data.rs` 之外仍引用三模块 → grep 修

如果遇到 unexpected reference，**STOP**，向 controller 报告 NEEDS_CONTEXT。

- [ ] **Step 4：cargo test --lib 验证 ~94 测全绿**

```powershell
cargo test --lib 2>&1 | Select-String -Pattern 'test result:'
```

预期：
- 删去 ~30 个 audio_dl/m4a_mux/mp4_box 模块测后，剩 ~94 测全 pass
- `download_kind_strings_are_stable` 通过新数组

- [ ] **Step 5：commit**

```bash
git add src/apps/canvas_video/mod.rs
git commit -m "feat(canvas_video): 删除 V5.D + V5.E-B+ audio-only 三模块 (audio_dl/m4a_mux/mp4_box)"
```

**Exit criteria**：cargo check + cargo test --lib 全绿，commit 落地，17 文件 + 3 目录已删。

---

## Task 3：cargo clippy + fmt 收尾

**Files**：可能小幅 fmt 改动（导入顺序 / 空行）

**Steps**：

- [ ] **Step 1：clippy --all-targets**

```powershell
cargo clippy --all-targets -- -D warnings 2>&1 | Select-String -Pattern 'warning|error'
```

预期：**零 warning + 零 error**。

如果有 warning：
- `unused_imports` → 删
- `dead_code` → 删函数（不允许 `#[allow(dead_code)]` 绕过）
- 其他 → 按 clippy 建议修

- [ ] **Step 2：cargo fmt**

```bash
cargo fmt
```

- [ ] **Step 3：cargo fmt --check 验证零 diff**

```bash
cargo fmt --check
```

退出码 0 = OK。

- [ ] **Step 4：如有改动，commit**

```bash
git status --short
# 如果有改动：
git add -A
git commit -m "chore(canvas_video): clippy + fmt 收尾"
# 如果无改动：跳过
```

**Exit criteria**：clippy 0 warning + fmt 0 diff。

---

## Task 4：真机 L10 单讲 smoke（main session 亲跑）

**前置**：sub_sessions 中 `canvas_video_bootstrap_88168_8329.json` 仍有效（ttl 1800s，必要时重新登录刷新）。

**Steps**：

- [ ] **Step 1：清空 tmp/v5f_smoke 旧工件（防止 batch skipped 逻辑误判）**

```powershell
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue tmp/v5f_smoke
New-Item -ItemType Directory -Force -Path tmp/v5f_smoke | Out-Null
```

- [ ] **Step 2：cargo build --release**

```bash
cargo build --release
```

预期：编译通过。如果失败，回 T3 修。

- [ ] **Step 3：单讲 smoke run + 时间测量**

注意：`course_id` 是**位置参数**，`tool_id` 用 `--tool-id`（不是 `--course/--tool`）。

```powershell
$sw = [System.Diagnostics.Stopwatch]::StartNew()
& .\target\release\sjtu.exe canvas-video download 88168 `
  --tool-id 8329 --lecture 10 --channel 0 `
  --to tmp/v5f_smoke --audio-only --concurrency 8 `
  2>tmp/v5f_smoke/L10_stderr.log | Out-File -Encoding utf8 tmp/v5f_smoke/L10_stdout.yaml
$sw.Stop()
"Total wall: $([math]::Round($sw.Elapsed.TotalSeconds,1)) s = $([math]::Round($sw.Elapsed.TotalMinutes,2)) min"
```

前置：`sub_sessions/canvas_video_bootstrap_<course>_<tool>.json` ttl 1800s，过期需先 `sjtu auth login` + `sjtu canvas-video list <course>` 触发 bootstrap 重建。

- [ ] **Step 4：验收指标**

读 `tmp/v5f_smoke/L10_stdout.yaml`，确认：
- `ok: true`
- `download_kind: mp4-full`
- `elapsed_ms ≤ 150000`（2.5 min）
- `bytes` 在 800-1000 MB 范围（916 MB ± 10%）
- `bytes_downloaded == bytes`（mp4-full 路径下两者恒等）
- `audio_path` 落盘 `tmp/v5f_smoke/*_ch0.m4a` 且大小 > 5 MB
- `mp4_kept: false`（audio_only && !keep_mp4 删 mp4）

```powershell
Get-Item tmp/v5f_smoke/*.m4a | Select-Object Name, @{N='SizeMB';E={[math]::Round($_.Length/1MB,1)}}
Get-Item tmp/v5f_smoke/*.mp4 -ErrorAction SilentlyContinue
```

预期 m4a 大小 ~22 MB，mp4 不存在。

- [ ] **Step 5：判定**
- ✅ elapsed ≤ 2.5 min + m4a 落盘 → 进 T5
- ⚠ elapsed 2.5-3 min → 接受（SJTU 带宽波动），进 T5
- ❌ elapsed > 3 min 或 ok=false → STOP，分析 stderr，回报 controller

**Exit criteria**：单讲 ≤ 2.5 min，验收 6 项全过。

---

## Task 5：真机 9 讲 batch + lessons + CLAUDE.md + 收尾 commit（main session 亲跑）

**Steps**：

- [ ] **Step 1：清空 tmp/v5f_batch**

```powershell
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue tmp/v5f_batch
New-Item -ItemType Directory -Force -Path tmp/v5f_batch | Out-Null
```

- [ ] **Step 2：batch 9 讲 run**

```powershell
$sw = [System.Diagnostics.Stopwatch]::StartNew()
& .\target\release\sjtu.exe canvas-video download 88168 `
  --tool-id 8329 --lectures 1-9 `
  --to tmp/v5f_batch --audio-only --concurrency 8 `
  2>tmp/v5f_batch/batch_stderr.log | Out-File -Encoding utf8 tmp/v5f_batch/batch_stdout.yaml
$sw.Stop()
"Total wall: $([math]::Round($sw.Elapsed.TotalSeconds,1)) s = $([math]::Round($sw.Elapsed.TotalMinutes,2)) min"
```

- [ ] **Step 3：验收**

读 `tmp/v5f_batch/batch_stdout.yaml`，确认：
- `ok: true`
- `data.total_planned: 9`
- `data.succeeded: 9`
- `data.failed_count: 0`
- `data.total_elapsed_ms ≤ 1500000`（25 min）
- `data.items[*].channels[*].download_kind: mp4-full`（9/9）
- 9 个 m4a 落盘 `tmp/v5f_batch/*.m4a` 且每个 > 5 MB

```powershell
Get-ChildItem tmp/v5f_batch/*.m4a | Select-Object Name, @{N='SizeMB';E={[math]::Round($_.Length/1MB,1)}}
Get-ChildItem tmp/v5f_batch/*.mp4 -ErrorAction SilentlyContinue  # 应为空
```

- [ ] **Step 4：判定 + 关 #42**
- ✅ ≤ 25 min + 9/9 mp4-full → 关 task #42 + 进 Step 5
- ⚠ 25-30 min → 仍关 #42 + 记 lessons
- ❌ > 30 min → STOP，分析 stderr，**不**关 #42，回报 controller

- [ ] **Step 5：lessons 落盘**

打开 `tasks/lessons.md`，新增条目（追加到现有内容末尾）：

```markdown
## V5.F 撤回 audio-only 整路 — 5 条工程范式（2026-05-12）

### 1. CDN audio-only endpoint 探测先于优化设计
V5.D + V5.E-B+ 共 5h 工时优化 audio Range merge，30s `curl -I` 11 variant probe 即能证伪 endpoint 存在
（all 404）。**新 CDN 接入：先 probe 再设计**。

### 2. HTTP/2 单 Client buffer bug 真机优先于纸面分析
reqwest #1276 + Tengine 128 stream limit + SETTINGS_INITIAL_WINDOW_SIZE 65535 三层叠加导致 V5.E-B+
4-Client H2 池真机 30.5 min/讲（4.7× 退化 vs V5.D）。**纸面优雅 ≠ 真机收益**，H2/H3 改造必须先小样真机验证。

### 3. mp4 moov 位置 hexdump 决定 stdin pipe 可行性
4 字节 box size + 4 字节 box type，紧跟 ftyp 的 box 若是：
- `moov` → faststart，可流式
- `mdat` → moov-end，ffmpeg stdin pipe 必报 "moov atom not found"
SJTU CDN 的 mp4 是 mdat-first，B.1 流式优化死路。

### 4. fail-soft 不应掩盖性能退化
V5.D `download_audio_only_to_file` 失败 → fail-soft 回退 V5.A mp4-full，但 main 路径正常时也偶发
~5% 慢退化未被察觉。**fail-soft 必须配合 envelope 计数器 + 显式 warn 计数**。

### 5. 工程权衡 vs 微优化
V5.A baseline ~2 min/讲，audio-only 理论上限 ~1.5 min/讲（22 MB 而非 916 MB），但工程复杂度爆炸。
**当 baseline 已达 80% 目标，剩 20% 优化成本可能 10×**。V5.F 选择接受 916 MB/讲 换稳定 2 min。
```

- [ ] **Step 6：CLAUDE.md 项目结构同步**

打开 `CLAUDE.md`，找到"项目结构"代码块，删除 `audio_dl/` / `m4a_mux/` / `mp4_box/` 相关行（如已有列出）。Status 当前阶段更新到 V5.F。

具体看现有 CLAUDE.md 文件，找 `apps/canvas_video/` 树状结构（如有），按实际删除条目。**如果没有列具体子模块树，则只更新"当前阶段"段：**

```markdown
### 当前阶段
- **已完成**：S0 / S1 / S2 / S3 (jwc MVP) / V5.F (canvas_video — mp4-full + ffmpeg 单路径)
- **V5 收尾**：撤回 V5.D + V5.E-B+ audio-only 整路，删 audio_dl/m4a_mux/mp4_box 三模块
- **下一步**：S4 — 待规划（jwc 课表 / 一卡通 / 通知）
```

- [ ] **Step 7：tasks/todo.md 更新**

打开 `tasks/todo.md`，找 V5.D / V5.E-B+ / V5.F 相关条目，标记 V5.F 完成 + task #42 关闭。

- [ ] **Step 8：最终 commit**

```bash
git add tasks/lessons.md CLAUDE.md tasks/todo.md
git commit -m "$(cat <<'EOF'
feat(canvas_video): V5.F 收尾 — 撤 audio-only 整路 + lessons + 项目结构同步

- 撤回 V5.D + V5.E-B+ 三模块（audio_dl/m4a_mux/mp4_box, ~1500 行）
- 单一路径 mp4-full + ffmpeg，单讲 ≤ 2.5 min / 9 讲 batch ≤ 25 min
- lessons.md 加 5 条范式（CDN probe / H2 真机 / hexdump / fail-soft / 权衡）
- 关 task #42
EOF
)"
```

- [ ] **Step 9：TaskUpdate 关 #42 + 标 V5.F 全完成**

```
TaskUpdate(42, status=completed)
TaskUpdate(63, status=completed)  # 单讲 smoke
TaskUpdate(64, status=completed)  # 9 讲 batch
TaskUpdate(65, status=completed)  # lessons + 收尾
```

**Exit criteria**：
- 9 讲 batch ≤ 25 min，9/9 mp4-full
- lessons + CLAUDE.md + todo.md 三处文档同步
- 最终 commit 落地
- task #42 关闭

---

## 失败回退预案

| 失败点 | 回退动作 |
|---|---|
| T1 cargo check 报 error | grep 找漏改的 import，手 fix |
| T2 cargo test 红 30 个以上 | 大概率是非 V5.D 模块意外引用，STOP 回报 |
| T3 clippy warning 持续 | `git revert` T1/T2，回 plan 重审 |
| T4 单讲 > 3 min | 检查 download.rs:48 `http1_only()` 仍存在；可能 SJTU CDN 限流，retry 2 次取最佳 |
| T5 batch > 30 min | 不关 #42，记 lessons "V5.A 路径在新环境退化"，开 V5.G 独立调研 task |

---

## 完成判定

✅ 全部满足：
- T1-T3 commits 落地，cargo test + clippy + fmt 全绿
- T4 单讲 ≤ 2.5 min
- T5 batch ≤ 25 min
- task #42 关闭
- lessons + CLAUDE.md 同步
