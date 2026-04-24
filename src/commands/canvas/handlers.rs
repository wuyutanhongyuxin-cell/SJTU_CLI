//! `sjtu canvas <sub>` handler：setup / whoami / today / upcoming。
//!
//! 时区策略：
//! - Canvas 响应时间全是 UTC（ISO8601 尾 Z）
//! - "今天" / "未来 N 天" 以 `Asia/Shanghai`（`+08:00`，不做 DST）为锚点
//! - 发往 Canvas 的 `start_date` / `end_date` 重新换成 UTC 字符串

use anyhow::Result;
use chrono::{DateTime, Duration, FixedOffset, NaiveTime, TimeZone, Utc};
use is_terminal::IsTerminal;
use std::io::{BufRead, Write};

use crate::apps::canvas::{auth, Client, PlannerItem, Submissions};
use crate::output::{render, Envelope, OutputFormat};

use super::data::{PlannerEntry, SetupData, TodayData, UpcomingData, WhoamiData};

/// SJTU 固定时区：`UTC+08:00`，无 DST。
fn sjtu_offset() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("const tz")
}

/// `sjtu canvas setup`：交互式粘贴 PAT → 落盘 → 立刻打一次 whoami 验证。
pub async fn cmd_setup(fmt: Option<OutputFormat>) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("`sjtu canvas setup` 需要在 TTY 环境交互粘贴 PAT（非 TTY 请直接写 `<config_dir>/sub_sessions/canvas_token.txt`）");
    }
    print!("粘贴 Canvas Personal Access Token（不会回显到终端的后续命令）: ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let path = auth::save_pat(&line)?;

    // 立刻用新 PAT 跑一次 whoami 做活性探测，避免落盘的 token 其实是错的。
    let login_id = match Client::connect() {
        Ok(c) => match c.whoami().await {
            Ok(p) => p.login_id,
            Err(e) => {
                eprintln!("[warn] PAT 已落盘但 whoami 验证失败：{e:#}");
                None
            }
        },
        Err(e) => {
            eprintln!("[warn] PAT 已落盘但 Client 构造失败：{e:#}");
            None
        }
    };
    render(
        Envelope::ok(SetupData {
            token_path: path.display().to_string(),
            login_id,
        }),
        fmt,
    )
}

/// `sjtu canvas whoami`：打 /users/self + /users/self/profile，合并输出。
pub async fn cmd_whoami(fmt: Option<OutputFormat>) -> Result<()> {
    let client = Client::connect()?;
    let profile = client.whoami().await?;
    let data: WhoamiData = profile;
    render(Envelope::ok(data), fmt)
}

/// `sjtu canvas today`：今天 00:00 → 明天 00:00（本地）窗口的 planner。
pub async fn cmd_today(include_done: bool, fmt: Option<OutputFormat>) -> Result<()> {
    let tz = sjtu_offset();
    let today_local = Utc::now().with_timezone(&tz).date_naive();
    let start = tz
        .from_local_datetime(&today_local.and_time(NaiveTime::MIN))
        .single()
        .expect("midnight 唯一");
    let end = start + Duration::days(1);
    let items = fetch_and_filter(&start, Some(&end), include_done).await?;
    render(
        Envelope::ok(TodayData {
            date_local: today_local.format("%Y-%m-%d").to_string(),
            include_done,
            returned: items.filtered.len(),
            total_raw: items.total_raw,
            items: items.filtered,
        }),
        fmt,
    )
}

/// `sjtu canvas upcoming --days N`：今天 00:00 本地 → +N 天窗口的 planner。
pub async fn cmd_upcoming(days: u32, include_done: bool, fmt: Option<OutputFormat>) -> Result<()> {
    if days == 0 {
        anyhow::bail!("--days 必须 > 0");
    }
    let tz = sjtu_offset();
    let today_local = Utc::now().with_timezone(&tz).date_naive();
    let start = tz
        .from_local_datetime(&today_local.and_time(NaiveTime::MIN))
        .single()
        .expect("midnight 唯一");
    let end = start + Duration::days(days as i64);
    let items = fetch_and_filter(&start, Some(&end), include_done).await?;
    render(
        Envelope::ok(UpcomingData {
            days,
            start_local: today_local.format("%Y-%m-%d").to_string(),
            end_local: end.date_naive().format("%Y-%m-%d").to_string(),
            include_done,
            returned: items.filtered.len(),
            total_raw: items.total_raw,
            items: items.filtered,
        }),
        fmt,
    )
}

struct Fetched {
    filtered: Vec<PlannerEntry>,
    total_raw: usize,
}

async fn fetch_and_filter(
    start: &DateTime<FixedOffset>,
    end: Option<&DateTime<FixedOffset>>,
    include_done: bool,
) -> Result<Fetched> {
    let client = Client::connect()?;
    let start_utc = start
        .with_timezone(&Utc)
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    let end_utc = end.map(|e| {
        e.with_timezone(&Utc)
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string()
    });
    let items = client
        .planner_items(&start_utc, end_utc.as_deref(), 100)
        .await?;
    let total_raw = items.len();
    let filtered: Vec<PlannerEntry> = items
        .into_iter()
        .filter(|it| include_done || Submissions::is_outstanding(it.submissions.as_ref()))
        .map(to_entry)
        .collect();
    Ok(Fetched {
        filtered,
        total_raw,
    })
}

/// PlannerItem → 扁平化的 PlannerEntry；时间本地化到 Asia/Shanghai。
fn to_entry(item: PlannerItem) -> PlannerEntry {
    let due_utc = item
        .plannable
        .as_ref()
        .and_then(|p| p.due_at.clone())
        .or_else(|| item.plannable_date.clone());
    let due_local = due_utc.as_deref().and_then(format_local);
    let points = item
        .plannable
        .as_ref()
        .and_then(|p| p.points_possible)
        .map(|v| {
            if v > 0.0 {
                format!("{v} 分")
            } else {
                "不计分".to_string()
            }
        });
    let subs = item.submissions.as_ref();
    let title = item.plannable.as_ref().and_then(|p| p.title.clone());
    let plannable_id = item
        .plannable
        .as_ref()
        .map(|p| p.id.clone())
        .unwrap_or(item.plannable_id);
    PlannerEntry {
        kind: item.plannable_type,
        title,
        course: item.context_name,
        course_id: item.course_id,
        plannable_id,
        due_at_utc: due_utc,
        due_at_local: due_local,
        points,
        submitted: subs.is_some_and(|s| s.submitted),
        missing: subs.is_some_and(|s| s.missing),
        graded: subs.is_some_and(|s| s.graded),
        html_url: item.html_url,
    }
}

/// `2026-04-25T15:59:59Z` → `2026-04-25 23:59`（本地时区）。
fn format_local(utc_str: &str) -> Option<String> {
    let parsed = DateTime::parse_from_rfc3339(utc_str).ok()?;
    Some(
        parsed
            .with_timezone(&sjtu_offset())
            .format("%Y-%m-%d %H:%M")
            .to_string(),
    )
}
