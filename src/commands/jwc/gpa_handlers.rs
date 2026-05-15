//! `sjtu jwc gpa-by-semester` ── 多学期 GPA 客户端循环聚合。
//!
//! 设计：枚举 (xnm, xqm) 组合，按 600ms throttle 串行调 N309131 两阶段 SP；
//! fail-soft 把"网络挂 / step1 拒绝 / items 空"三类失败都装进 `failed` 数组，
//! 整体 exit code 仍为 0（agent 通过 `failed.len()` 自决是否重试）。

use std::time::Duration;

use anyhow::Result;
use chrono::Datelike;

use crate::apps::jwc::{Client, GpaRank, GpaScope, LOGIN_URL};
use crate::auth::cas::with_cas_refresh;
use crate::output::{render, Envelope, OutputFormat};

use super::data::{GpaBySemesterData, SemesterFailure, SemesterGpa, SemesterKey};

/// 与 T1 schedule 8-周循环对齐的节流（毫秒）。
const SEMESTER_QUERY_THROTTLE_MS: u64 = 600;
/// xqm 枚举：3=秋季 / 12=春季 / 16=夏季。
const XQM_LIST: &[&str] = &["3", "12", "16"];

/// `sjtu jwc gpa-by-semester [--scope] [--rank] [--xnm-from] [--xnm-to]`。
///
/// 默认 `xnm-from` = 当年 - 3（覆盖 4 年制本科），`xnm-to` = 当年。
/// 非 4 年学制需手给。
pub async fn cmd_gpa_by_semester(
    scope: GpaScope,
    rank: GpaRank,
    xnm_from: Option<u32>,
    xnm_to: Option<u32>,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let (from, to) = resolve_xnm_range(xnm_from, xnm_to);
    let requested = enumerate_semesters(from, to);

    let requested_for_op = requested.clone();
    let (succeeded, failed) = with_cas_refresh("jwc", LOGIN_URL, |session| {
        let requested = requested_for_op.clone();
        async move {
            let client = Client::from_session(session)?;
            let mut succeeded: Vec<SemesterGpa> = Vec::new();
            let mut failed: Vec<SemesterFailure> = Vec::new();

            for key in &requested {
                let xnxq = format!("{}{}", key.xnm, key.xqm);
                let res = client.gpa(scope, rank, Some(&xnxq), Some(&xnxq)).await;
                tokio::time::sleep(Duration::from_millis(SEMESTER_QUERY_THROTTLE_MS)).await;
                match res {
                    Ok(mut env) if !env.items.is_empty() => {
                        let mut g = env.items.remove(0);
                        g.fill_parsed();
                        let mut sg: SemesterGpa = (&g).into();
                        sg.xnm = key.xnm.clone();
                        sg.xqm = key.xqm.clone();
                        succeeded.push(sg);
                    }
                    Ok(_) => failed.push(SemesterFailure {
                        xnm: key.xnm.clone(),
                        xqm: key.xqm.clone(),
                        reason: "items 空（疑似未到统计时间或该学期无成绩）".into(),
                    }),
                    Err(e) => {
                        // SubSessionStale 必须上抛触发 retry，不能装进 failed 吞掉
                        if e.downcast_ref::<crate::error::SjtuCliError>()
                            .map(|err| {
                                matches!(err, crate::error::SjtuCliError::SubSessionStale(_))
                            })
                            .unwrap_or(false)
                        {
                            return Err(e);
                        }
                        failed.push(SemesterFailure {
                            xnm: key.xnm.clone(),
                            xqm: key.xqm.clone(),
                            reason: format!("{e:#}"),
                        });
                    }
                }
            }
            Ok::<_, anyhow::Error>((succeeded, failed))
        }
    })
    .await?;

    let data = GpaBySemesterData {
        scope: scope_str(scope),
        rank: rank_str(rank),
        xnm_from: from,
        xnm_to: to,
        requested,
        succeeded,
        failed,
    };
    render(Envelope::ok(data), fmt)
}

fn resolve_xnm_range(xnm_from: Option<u32>, xnm_to: Option<u32>) -> (u32, u32) {
    let now_year = chrono::Local::now().year() as u32;
    let to = xnm_to.unwrap_or(now_year);
    let from = xnm_from.unwrap_or(now_year.saturating_sub(3));
    (from, to)
}

fn enumerate_semesters(xnm_from: u32, xnm_to: u32) -> Vec<SemesterKey> {
    let mut out = Vec::with_capacity(((xnm_to - xnm_from + 1) * 3) as usize);
    for xnm in xnm_from..=xnm_to {
        for xqm in XQM_LIST {
            out.push(SemesterKey {
                xnm: xnm.to_string(),
                xqm: (*xqm).to_string(),
            });
        }
    }
    out
}

fn scope_str(s: GpaScope) -> &'static str {
    match s {
        GpaScope::HxKc => "hxkc",
        GpaScope::QbKc => "qbkc",
    }
}

fn rank_str(r: GpaRank) -> &'static str {
    match r {
        GpaRank::NjZy => "njzy",
        GpaRank::Nj => "nj",
        GpaRank::Bj => "bj",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerate_semesters_4_years_yields_12() {
        let xs = enumerate_semesters(2023, 2026);
        assert_eq!(xs.len(), 12);
        assert_eq!(xs[0].xnm, "2023");
        assert_eq!(xs[0].xqm, "3");
        assert_eq!(xs[11].xnm, "2026");
        assert_eq!(xs[11].xqm, "16");
    }

    #[test]
    fn resolve_xnm_range_defaults_to_4_year_window() {
        let now = chrono::Local::now().year() as u32;
        let (from, to) = resolve_xnm_range(None, None);
        assert_eq!(to, now);
        assert_eq!(from, now - 3);
    }

    #[test]
    fn resolve_xnm_range_respects_explicit() {
        let (from, to) = resolve_xnm_range(Some(2020), Some(2024));
        assert_eq!(from, 2020);
        assert_eq!(to, 2024);
    }
}
