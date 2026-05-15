//! `sjtu jwc next`：接下来 within 天内前 limit 节课（跨周串行拉取 + datetime 排序）。

use anyhow::Result;
use chrono::{Duration, Local, NaiveDate};

use crate::apps::jwc::{Client, LOGIN_URL};
use crate::auth::cas::with_cas_refresh;
use crate::output::{render, Envelope, OutputFormat};

use super::data::{NextData, NextItem};
use super::schedule_helpers::{
    combine_dt, expand_jc, filter_kb_in_week, parse_xqj, weeks_to_fetch_for_within,
};

/// `sjtu jwc next`：接下来 within 天内前 limit 节课。
pub async fn cmd_next(
    xnm: Option<String>,
    xqm: Option<String>,
    within: u8,
    limit: u8,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let xnm_q = xnm.clone();
    let xqm_q = xqm.clone();
    let now = Local::now().naive_local();
    let today = now.date();

    let (cw, fetched_weeks, mut all_items) = with_cas_refresh("jwc", LOGIN_URL, |session| {
        let xnm = xnm_q.clone();
        let xqm = xqm_q.clone();
        async move {
            let client = Client::from_session(session)?;
            let cw = client
                .infer_current_week(xnm.as_deref(), xqm.as_deref())
                .await?;

            // 学期外直接返回，不拉周数据
            if cw == 0 || cw > 18 {
                return Ok::<_, anyhow::Error>((cw, vec![], vec![]));
            }

            let n_weeks = weeks_to_fetch_for_within(within);
            let mut fetched_weeks: Vec<u8> = Vec::new();
            let mut all_items: Vec<NextItem> = Vec::new();

            for offset in 0..n_weeks {
                let zs = cw.saturating_add(offset);
                if zs > 18 {
                    break;
                }
                fetched_weeks.push(zs);
                let sched = client
                    .schedule_by_week(xnm.as_deref(), xqm.as_deref(), zs)
                    .await?;
                // ⚠️ RqAzc.xqj 是 Option<u8>，用 == Some(1)，不是 string compare
                let week_mon: Option<NaiveDate> = sched
                    .rqazc_list
                    .iter()
                    .find_map(|r| r.rq.as_deref().filter(|_| r.xqj == Some(1)))
                    .or_else(|| sched.rqazc_list.first().and_then(|r| r.rq.as_deref()))
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

                let Some(week_mon) = week_mon else {
                    continue;
                };
                let filtered = filter_kb_in_week(&sched.kb_list, zs);

                for k in filtered.iter() {
                    let xqj = parse_xqj(k.xqj.as_deref());
                    if !(1..=7).contains(&xqj) {
                        continue;
                    }
                    let (jc_list, clock_list) = expand_jc(k.old_jc);
                    if jc_list.is_empty() {
                        continue;
                    }
                    let course_date = week_mon + Duration::days((xqj - 1) as i64);
                    if course_date < today || (course_date - today).num_days() > within as i64 {
                        continue;
                    }
                    let start_str = clock_list
                        .first()
                        .map(|(s, _)| s.clone())
                        .unwrap_or_default();
                    let end_str = clock_list
                        .last()
                        .map(|(_, e)| e.clone())
                        .unwrap_or_default();
                    let start_dt = combine_dt(course_date, &start_str);
                    // 今天已结束的课跳过
                    if course_date == today && start_dt <= now {
                        continue;
                    }
                    all_items.push(NextItem {
                        kcmc: k.kcmc.clone(),
                        datetime_start: start_dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
                        datetime_end: combine_dt(course_date, &end_str)
                            .format("%Y-%m-%dT%H:%M:%S")
                            .to_string(),
                        week: zs,
                        xqj,
                        jc_list,
                        cdmc: k.cdmc.clone(),
                        xm: k.xm.clone(),
                    });
                }
            }
            Ok((cw, fetched_weeks, all_items))
        }
    })
    .await?;

    // fail-soft: 学期外直接返回 hint
    if cw == 0 || cw > 18 {
        let hint = if cw == 0 {
            "学期未开始"
        } else {
            "学期已结束 / 假期"
        };
        return render(
            Envelope::ok(NextData {
                xnm,
                xqm,
                current_week: cw,
                within_days: within,
                limit,
                fetched_weeks: vec![],
                hint: Some(hint.into()),
                items: vec![],
            }),
            fmt,
        );
    }

    all_items.sort_by(|a, b| a.datetime_start.cmp(&b.datetime_start));
    all_items.truncate(limit as usize);

    render(
        Envelope::ok(NextData {
            xnm,
            xqm,
            current_week: cw,
            within_days: within,
            limit,
            fetched_weeks,
            hint: None,
            items: all_items,
        }),
        fmt,
    )
}
