# Canvas 课堂视频语音转录调研（V5.C）

> **状态**：调研文档（非实装）
> **日期**：2026-05-10
> **目的**：为 V6 `sjtu canvas-video transcribe` 子命令选定底层语音转录引擎
> **输出**：本文档 + V6 实装计划在用户拍板后另写
> **范围**：不写代码、不跑真实模型测 WER（留 V6 决策时做）；只到"V6 该装哪个项目"的明确推荐 + 集成草图

---

## 1. 背景：为什么自带语音转录而不是用 Canvas 字幕

### 1.1 Canvas 字幕端点（`videSrtUrl`）实测不可用

CP-V0 调研（`tasks/canvas_video_investigation.md` §6）抓 `getVodVideoInfos` 响应字段时观测到 `videSrtUrl` 字段：

- 在我们已注册课程的所有 18 讲样本上，`videSrtUrl` 全部为 `null`（即没有上传过任何字幕轨）
- 即便有的课程恰好有字幕，用户原话是"有出入"，准确度差到不能直接给学习用

### 1.2 学习场景对转录稳定度的硬要求

CP-V4 已实装 audio-only 抽流（`--audio-only` 走 ffmpeg `-vn -c:a copy` 抽 m4a，单讲约 20MB，40× 压缩比）。下一步的"听课/复习"用户路径：
- 听完音频后想搜某个术语 → 需要文本可索引
- 想重温某个公式定义 → 需要时间戳定位
- 想喂给 LLM 做总结 → 需要清晰中文文本

任何低于 95% CER（中文词错率）的转录都会让 LLM 总结环节出现编造，体验破坏。所以底层转录引擎的选择对整个 transcribe 子命令的可用性是决定性的。

### 1.3 V6 设计原则继承自 V5

- **本地优先**：跟 V5.A 缓存一样，`transcribe` 子命令应当本地跑（不上传音频到第三方），保护学生隐私
- **可降级**：CPU 也能跑（让没 GPU 的同学也能用），有 GPU 时自动加速
- **无新 Rust 依赖**：跟 V4 ffmpeg 范式一样，调外部子进程（`std::process::Command + tokio::task::spawn_blocking`），不引 Python 运行时进 sjtu-cli 二进制

---

## 2. 候选 4 家

按"成熟度 + 中文友好度 + 集成成本"轴选。

### 2.1 faster-whisper（CTranslate2 加速 Whisper）

- **底层模型**：OpenAI Whisper large-v3（2023 年 11 月发布，680k 小时训练数据，多语言含中文）
- **加速引擎**：CTranslate2（INT8 量化 + GPU/CPU 并行优化）
- **仓库**：`guillaumekln/faster-whisper`（GitHub 主仓）+ `Softcatala/whisper-ctranslate2`（CLI 封装）
- **对比基准**：作者 README 称对原生 `openai-whisper` 同硬件 / 同模型 ~4× 速度（large-v3 上 GPU），CPU 上 ~2-3×。large-v3 + GPU 大约 5-10× real-time（即 1 小时音频 6-12 分钟搞定）
- **中文质量**：Whisper large-v3 在中英文混说场景表现稳；CER 业界报告 5-10% 区间（视音频质量），课堂教学视频清晰度好时偏低端
- **字幕输出**：原生支持 SRT / VTT / TSV / TXT（`whisper-ctranslate2 --output_format srt`），跟 V4 audio-only 产物 `_ch0.m4a` 同目录同 stem 自然落 `_ch0.srt`
- **部署门槛**：用户跑 `pipx install faster-whisper-cli` 或 `pip install -U whisper-ctranslate2`；首次跑会自动下 large-v3 模型 ~3GB（HuggingFace 镜像 / modelscope 都有）
- **成本**：开源 MIT / 完全免费

### 2.2 WhisperX（词级时间戳 + VAD）

- **底层模型**：仍是 Whisper（任何尺寸），但加 wav2vec2 作 forced alignment 出**词级**时间戳
- **加速引擎**：faster-whisper 后端（同 §2.1）
- **仓库**：`m-bain/whisperX`（GitHub）
- **独特价值**：句级时间戳（whisper 默认）→ 词级时间戳（WhisperX 加的），对"想跳到老师讲到'卷积'那一秒"的精确定位场景有用
- **中文质量**：跟 faster-whisper 一致（同模型）；词级对齐用 wav2vec2-large-xlsr-53 中文头
- **字幕输出**：SRT 含词级 timestamps（每行一个词）
- **部署门槛**：比 faster-whisper 高一截 —— **强 GPU 依赖**（pyannote VAD + wav2vec2 都吃显存），CPU 跑慢到不可接受。模型下载量比 faster-whisper 多一倍（whisper + wav2vec2 + pyannote）
- **成本**：开源 BSD / 免费

### 2.3 paraformer / FunASR（达摩院专精普通话）

- **底层模型**：达摩院自研 Paraformer-Large-zh（非自回归 + ParaFormer 架构，专门为普通话训练）
- **框架**：FunASR（modelscope 生态）
- **仓库**：`alibaba-damo-academy/FunASR`（GitHub）+ modelscope 镜像
- **独特价值**：纯中文场景下业界中文 SOTA（AISHELL-1 / AISHELL-2 上跑赢 Whisper-large-v3 几个百分点；公开 benchmark）
- **中文质量**：大概率最优 —— 但对中英文夹杂（教学常态：算子名 / 术语 / API 名都英文）的鲁棒性比 Whisper 弱
- **字幕输出**：原生有 SRT export（`funasr` CLI / Python API），但 CLI 工具链的丰富度不及 whisper 生态
- **部署门槛**：modelscope 生态偏闭 —— 模型下载、版本切换、依赖版本对齐都得走 modelscope SDK（不像 HuggingFace 通用）；Python 生态对 Windows 用户不太友好
- **成本**：开源 Apache 2.0 / 模型权重免费

### 2.4 商用云 API（讯飞 / 火山 / 腾讯）

- **代表**：讯飞实时语音转写 / 火山引擎语音技术 / 腾讯云 ASR
- **质量**：中文 SOTA 一档（用工业级声学模型 + 大规模行业语料训练）
- **接入**：HTTPS REST POST 一段音频 → JSON 含 transcript + timestamps
- **独特价值**：零本地部署、无显卡门槛、按秒计费（讯飞约 ¥30/小时音频）
- **缺陷**：
  - 必须联网（学校内网 / 节假日断网都是问题）
  - 必须企业账号 + 实名认证 + 申请 API key（学生用户上手成本高）
  - 隐私：上传课堂音频到第三方厂商不符合 V6 "本地优先"原则
  - 单课程一学期 18 讲 × 1.5h × ¥30 ≈ ¥800，对学生来说不便宜
- **成本**：按用量计费

---

## 3. 对比维度总表

| 维度 | faster-whisper | WhisperX | paraformer | 商用云 |
|---|---|---|---|---|
| **模型大小** | ~3GB（large-v3 INT8 量化） | ~5GB（+wav2vec2 + pyannote） | ~2GB（Paraformer-Large-zh） | 0（云端） |
| **速度（GPU）** | 5-10× real-time | 4-8× real-time | ~10× real-time | 1× real-time（接口限速） |
| **速度（CPU）** | 0.3-0.5× real-time（large-v3）/ 1-3× real-time（base / small） | 不可行 | 0.5-1× real-time | N/A |
| **中文质量** | 好（CER 5-10%） | 好（同 faster-whisper） | 优（CER 4-7%，纯中文 benchmark 略胜） | 优（CER 4-6%） |
| **中英混说** | 优 | 优 | 中（专精普通话，英文 token 较弱） | 优 |
| **字幕格式** | SRT / VTT / TSV / TXT 现成 | SRT 词级 | SRT（funasr 出） | JSON → 自己拼 SRT |
| **部署门槛** | 中（pipx 一行 + 模型自动下） | 高（GPU + 多模型） | 中高（modelscope 生态） | 低（API key） |
| **离线** | ✅ | ✅ | ✅ | ❌ |
| **隐私** | ✅（本地） | ✅ | ✅ | ❌（音频上云） |
| **CPU 兜底** | ✅（base / small 模型可用） | ❌ | ✅ | N/A |
| **成本** | 0 | 0 | 0 | ¥30/h |
| **生态/社区** | 大（whisper 系生态） | 大（whisper 系 + alignment） | 中（modelscope 内） | 大（厂商支持） |

---

## 4. 推荐：faster-whisper + large-v3

### 4.1 综合最优理由

1. **本地 + 免费 + 离线**：完美对齐 V6 "本地优先"原则
2. **中英混说稳**：教学场景常态（算子名 / 术语 / API 名英文 + 普通话讲解），Whisper 多语言基底比 paraformer 普通话单语训练更适配
3. **SRT 现成**：跟 V4 audio-only 产物天然耦合（`_ch0.m4a` → `_ch0.srt`）
4. **CPU 可降级**：large-v3 在 CPU 上慢但能跑（base / small 模型 1-3× real-time），照顾没 GPU 的同学
5. **生态成熟**：whisper 系工具 / 文档 / 中文社区资料丰富，遇到问题好查
6. **依赖简单**：`pipx install whisper-ctranslate2` 一行装好，不引 modelscope / pyannote 等额外生态

### 4.2 不取其他 3 家的具体理由

- **WhisperX**：词级时间戳是锦上添花不是必需，强 GPU 依赖把"没 GPU 的同学"挡在门外，违反 CPU 兜底原则
- **paraformer**：纯中文场景略胜不足以覆盖中英混说劣势 + modelscope 生态闭环成本
- **商用云**：违反"隐私 + 离线"原则；按 18 讲 ¥800 学期成本对学生不友好

---

## 5. V6 `transcribe` 子命令集成草图

### 5.1 命令形态

```
sjtu canvas-video transcribe <m4a-or-mp4> [--model <name>] [--language zh] [--out <srt>]
sjtu canvas-video transcribe --batch <dir> [--model <name>] [--language zh]
```

参数：
- `<m4a-or-mp4>`：单文件输入（mp4 自动调 ffmpeg 抽流）
- `--batch <dir>`：批量目录模式，扫 *.m4a / *.mp4，每文件一个 SRT
- `--model`：默认 `large-v3`；可选 `base` / `small` / `medium` / `large-v3` / `large-v3-turbo`
- `--language`：默认 `zh`，关闭多语言侦测加快 + 减误判
- `--out`：默认 `<input-dir>/<stem>.srt`

### 5.2 调用方式（沿用 V4 ffmpeg 范式）

```rust
// src/apps/canvas_video/whisper.rs（新建，~80 行）
pub async fn ensure_whisper() -> Result<()>
pub async fn transcribe_to_srt(audio: &Path, srt: &Path, model: &str, language: &str) -> Result<()>
```

- `std::process::Command + tokio::task::spawn_blocking` 调外部 `whisper-ctranslate2` 子进程
- 不引 Python 运行时进 sjtu-cli 二进制
- `ensure_whisper` 仿 `ensure_ffmpeg`：缺工具时给清晰 install hints
- 子进程 stderr 进度行进 stderr（不污染 stdout 的 envelope，跟 V4 ffmpeg 抽流一致）

### 5.3 批量模式复用 V4 框架

`--batch <dir>` 复用 V4 fail-soft：
- 扫目录得 *.m4a 列表
- 单文件失败计入 errors 不阻塞
- 已存在的 .srt（dest 已存在 size > 0）→ skip
- envelope 出 `transcribed_count / failed_count / skipped_count + total_elapsed_ms`

### 5.4 性能预算（V6 实装时校准）

V5.B 18 讲 audio-only 跑完后能拿到样本。粗估：
- GPU 5-10× real-time → 18 × 1.5h / 7× ≈ 4 小时
- CPU large-v3 0.3× real-time → 18 × 1.5h / 0.3 ≈ 90 小时（实际不可用）
- CPU small 1-2× real-time → 18 × 1.5h / 1.5 ≈ 18 小时（极限可用，用户得过夜跑）

→ **V6 配合策略**：默认 GPU 检测（CUDA / MPS / DirectML），fall back 到 CPU 时建议改用 small 模型 + 警告字幕质量降级。

---

## 6. 风险与待 V6 验证

### 6.1 已知风险

- **18 讲 large-v3 单 m4a 单机 wall time** 待 V5.B 跑完才能基准（V5.A 节省的 LTI launch 时间 ≪ 转录时间，整体 V5/V6 体验主导项是转录速度）
- **中文专业词汇覆盖**：日语语言学专题课的"日语形态学 / 万叶集"等术语在 Whisper 训练数据中可能稀疏 → 需要 V6 真机抽样人工核对
- **数学公式 / 黑板内容**：纯语音转录无法读黑板，公式只能靠老师念出来；课堂视频的 ch1 PPT 机位的 OCR 是另一条独立路径（不在 V6 范围）
- **超长音频 OOM**：1.5 小时 m4a 单批送进 large-v3 可能 OOM（取决于 GPU 显存）；whisper-ctranslate2 内置 chunk + concat，但 chunk 边界可能丢词 / 重叠 → 需要 V6 调参

### 6.2 V6 决策时再做的事

1. 用 V5.B 产物 18 个 m4a 跑 small / medium / large-v3 三档，比 CER + wall time
2. 决定默认模型（看 GPU 普及度 + 用户硬件画像）
3. 决定是否上 WhisperX 出词级时间戳作为 `--word-level` opt-in（不默认开 GPU 门槛）
4. 评估 `--diarization`（说话人分离）—— 课堂场景一般只有一个老师讲，多说话人不是核心需求；先不上

---

## 7. 不取的方案及理由

| 方案 | 不取理由 |
|---|---|
| Canvas 字幕（`videSrtUrl`）| 实测全 null + 用户报"有出入" |
| Python 运行时直接嵌入 sjtu-cli 二进制 | 引来巨依赖 + cross-platform 部署变复杂 + 跟 V4 ffmpeg 范式不一致 |
| GPU-only 路径 | 排除没 GPU 的同学，违反 V6 平等使用原则 |
| 上传第三方云 ASR | 违反隐私优先原则 + 单课程一学期 ¥800 成本对学生不友好 |
| 实时流式 ASR | 课堂录播是文件场景不是流场景，不需要流式；流式实装会显著加复杂度 |
| 自训中文 ASR 模型 | 远超 V6 范围 + 学术级工程量；用现成 Whisper / paraformer 是正解 |

---

## 8. 推荐落地决策

**对 V6 transcribe 子命令的明确推荐**：

> 选 **faster-whisper + Whisper large-v3**，通过 `whisper-ctranslate2` CLI 子进程调用。GPU 自动加速（CUDA / MPS / DirectML），CPU 降级到 small 模型并警告。批量模式复用 V4 fail-soft + skip-via-fs 框架。

V6 实装计划在 V5.B 跑完拿到 18 讲音频基准 + 用户拍板后另写 spec。
