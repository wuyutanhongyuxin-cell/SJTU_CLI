//! `sjtu jwc today / week / next`：T9 衍生命令 handler。
//!
//! 数据流：infer_current_week → schedule_by_week → oldzc 过滤 → period_clock join。
//! helpers（filter / expand_jc / parse_xqj 等）在 schedule_helpers.rs。

use anyhow::Result;
use chrono::Local;

use crate::apps::jwc::Client;
use crate::output::{render, Envelope, OutputFormat};

use super::data::{TodayData, TodayItem, WeekData};
use super::schedule_helpers::{
    expand_jc, filter_kb_in_week, iso_weekday, parse_xqj, render_day_grid, render_week_grid,
};

/// `sjtu jwc today`：今日剩余的课。
pub async fn cmd_today(
    xnm: Option<String>,
    xqm: Option<String>,
    grid: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let client = Client::connect().await?;
    let cw = client
        .infer_current_week(xnm.as_deref(), xqm.as_deref())
        .await?;
    let today = Local::now().date_naive();
    let today_weekday = iso_weekday(today);
    let today_iso = today.format("%Y-%m-%d").to_string();

    // fail-soft: 学期外直接返回 hint
    if cw == 0 || cw > 18 {
        let hint = if cw == 0 {
            "学期未开始"
        } else {
            "学期已结束 / 假期"
        };
        return render(
            Envelope::ok(TodayData {
                xnm,
                xqm,
                current_week: cw,
                today_iso,
                today_weekday,
                hint: Some(hint.into()),
                items: vec![],
            }),
            fmt,
        );
    }

    let sched = client
        .schedule_by_week(xnm.as_deref(), xqm.as_deref(), cw)
        .await?;
    let filtered = filter_kb_in_week(&sched.kb_list, cw);
    let now_time = Local::now().time();

    let mut items: Vec<TodayItem> = filtered
        .iter()
        .filter_map(|k| {
            let xqj = parse_xqj(k.xqj.as_deref());
            if xqj != today_weekday {
                return None;
            }
            let (jc_list, clock_list) = expand_jc(k.old_jc);
            if clock_list.is_empty() {
                return None;
            }
            // 跳过今天已结束的课
            if let Some((_, last_end)) = clock_list.last() {
                if let Ok(t) = chrono::NaiveTime::parse_from_str(last_end, "%H:%M") {
                    if t <= now_time {
                        return None;
                    }
                }
            }
            Some(TodayItem {
                kcmc: k.kcmc.clone(),
                xqj,
                jc_list,
                clock_list,
                jcor_fallback: k.jcor.clone(),
                cdmc: k.cdmc.clone(),
                xm: k.xm.clone(),
                kch: k.kch.clone(),
            })
        })
        .collect();
    items.sort_by_key(|i| i.jc_list.first().copied().unwrap_or(99));

    if grid {
        print!("{}", render_day_grid(&items));
        return Ok(());
    }

    render(
        Envelope::ok(TodayData {
            xnm,
            xqm,
            current_week: cw,
            today_iso,
            today_weekday,
            hint: None,
            items,
        }),
        fmt,
    )
}

/// `sjtu jwc week`：指定周（或当前周）整周课表。
pub async fn cmd_week(
    xnm: Option<String>,
    xqm: Option<String>,
    zs: Option<u8>,
    grid: bool,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let client = Client::connect().await?;
    let cw = client
        .infer_current_week(xnm.as_deref(), xqm.as_deref())
        .await?;
    let query_zs = zs.unwrap_or(cw.clamp(1, 18));

    let sched = client
        .schedule_by_week(xnm.as_deref(), xqm.as_deref(), query_zs)
        .await?;
    let filtered = filter_kb_in_week(&sched.kb_list, query_zs);

    let mut items: Vec<TodayItem> = filtered
        .iter()
        .filter_map(|k| {
            let xqj = parse_xqj(k.xqj.as_deref());
            let (jc_list, clock_list) = expand_jc(k.old_jc);
            if jc_list.is_empty() {
                return None;
            }
            Some(TodayItem {
                kcmc: k.kcmc.clone(),
                xqj,
                jc_list,
                clock_list,
                jcor_fallback: k.jcor.clone(),
                cdmc: k.cdmc.clone(),
                xm: k.xm.clone(),
                kch: k.kch.clone(),
            })
        })
        .collect();
    items.sort_by_key(|i| (i.xqj, i.jc_list.first().copied().unwrap_or(99)));

    let hint = if cw == 0 {
        Some("学期未开始".into())
    } else if cw > 18 {
        Some("学期已结束 / 假期".into())
    } else {
        None
    };

    if grid {
        print!("{}", render_week_grid(&sched.rqazc_list, &items));
        return Ok(());
    }

    render(
        Envelope::ok(WeekData {
            xnm,
            xqm,
            current_week: cw,
            query_zs,
            rqazc_list: sched.rqazc_list,
            hint,
            items,
        }),
        fmt,
    )
}
