//! `sjtu canvas-video download --lectures <SPEC>` handler（CP-V4）：批量下载。
//!
//! 设计要点：
//! - mp4 URL 含 `key=` 时效签名（1-3h 过期），36 个文件按 ~5 MB/s 下载估 3+ 小时必过期
//!   ⇒ **临用临取**：每讲每 channel 下载前才调 `get_video_info` 拿新 URL
//! - **fail-soft**：单讲失败进 `BatchEntry.errors`，不阻塞后续讲
//! - **断点续传**：dest 已存在且 size > 0 → skip（写 `status=skipped`）；不重发 get_video_info 也不抽流
//! - **进度行**：`[k/N ch{0,1}] name` 打到 stderr（不污染 stdout 的 envelope）
//! - **--audio-only 早爆**：批量开始前 ensure_ffmpeg，缺则报错不进入下载阶段

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;

use crate::apps::canvas_video::{Client, LectureVideo};
use crate::output::{render, Envelope, OutputFormat};

use super::data::{BatchData, BatchEntry, ChannelOutput};
use super::download_handler::{download_one_channel, prep};
use super::handlers::{list_audited_sorted, redact_or_full, safe_filename};
use super::lectures_spec::parse_lectures_spec;

/// 批量参数簇（避免 cli/dispatch 处出现 12 参数的函数签名）。
pub struct BatchArgs {
    pub course_id: u64,
    pub tool_id: u64,
    pub lectures_spec: String,
    pub to_dir: PathBuf,
    pub all_channels: bool,
    pub channel: i32,
    pub concurrency: usize,
    pub audio_only: bool,
    pub keep_mp4: bool,
    pub with_identity: bool,
    pub fmt: Option<OutputFormat>,
}

/// 批量下载入口。
pub async fn cmd_download_batch(args: BatchArgs) -> Result<()> {
    let started = Instant::now();
    prep(args.audio_only, &args.to_dir).await?;
    let client = Client::connect(args.course_id, args.tool_id).await?;
    let audited = list_audited_sorted(&client).await?;
    let seq_list = parse_lectures_spec(&args.lectures_spec, audited.len() as u32)?;
    let channels: Vec<i32> = if args.all_channels {
        vec![0, 1]
    } else {
        vec![args.channel]
    };

    let mut items: Vec<BatchEntry> = Vec::with_capacity(seq_list.len());
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut total_bytes = 0u64;
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
            items,
        }),
        args.fmt,
    )
}

/// 单讲循环：对所选 channels 各下一遍，fail-soft。无任何成功且无 skip → status=failed。
async fn download_one_lecture(
    client: &Client,
    target: &LectureVideo,
    seq: u32,
    channels: &[i32],
    args: &BatchArgs,
) -> BatchEntry {
    let mut chs: Vec<ChannelOutput> = Vec::with_capacity(channels.len());
    let mut errors: Vec<String> = Vec::new();
    let mut video_name: Option<String> = None;
    let mut all_skipped = true;
    for &ch in channels {
        if let Some(out) = check_skip(target, ch, args) {
            chs.push(out);
            continue;
        }
        all_skipped = false;
        match download_one_channel(
            client,
            target,
            ch,
            &args.to_dir,
            args.concurrency,
            args.audio_only,
            args.keep_mp4,
            args.with_identity,
        )
        .await
        {
            Ok((fetch, out)) => {
                if video_name.is_none() {
                    video_name = fetch.video_name;
                }
                chs.push(out);
            }
            Err(e) => {
                let msg = format!("ch{ch}: {}", e.to_string().lines().next().unwrap_or(""));
                eprintln!("  ⚠ {msg}");
                errors.push(msg);
            }
        }
    }
    let status = derive_status(channels.len(), &chs, &errors, all_skipped);
    BatchEntry {
        seq,
        video_name: video_name.or_else(|| Some(target.video_name.clone())),
        video_id_redacted: redact_or_full(&target.video_id, args.with_identity),
        status,
        channels: chs,
        errors,
    }
}

/// 已存在 final 产物（audio_only 看 m4a，否则看 mp4）→ 构造 skip ChannelOutput。
/// 不调 get_video_info（避免无谓网络调用 + 过期签名爆）。
fn check_skip(target: &LectureVideo, channel: i32, args: &BatchArgs) -> Option<ChannelOutput> {
    let stem = target.video_name.as_str().trim();
    let safe_stem = safe_filename(if stem.is_empty() { "video" } else { stem });
    let mp4 = args.to_dir.join(format!("{safe_stem}_ch{channel}.mp4"));
    let m4a = args.to_dir.join(format!("{safe_stem}_ch{channel}.m4a"));
    let final_path = if args.audio_only { &m4a } else { &mp4 };
    let meta = std::fs::metadata(final_path).ok()?;
    if meta.len() == 0 {
        return None;
    }
    let mp4_kept = !args.audio_only || args.keep_mp4 && mp4.exists();
    eprintln!("  ✓ ch{channel} 已存在 ({} bytes) → skip", meta.len());
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
    })
}

/// `ok` / `skipped` / `partial` / `failed` 派生：根据每路结果数。
fn derive_status(want: usize, got: &[ChannelOutput], errs: &[String], all_skipped: bool) -> String {
    if all_skipped && got.len() == want {
        "skipped".into()
    } else if errs.is_empty() && got.len() == want {
        "ok".into()
    } else if got.is_empty() {
        "failed".into()
    } else {
        "partial".into()
    }
}
