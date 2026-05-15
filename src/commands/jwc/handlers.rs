//! `sjtu jwc <sub>` 的只读 handler。
//!
//! 已实装：grades (§2.1) / schedule (§2.2) / gpa (§2.3 两阶段) / exams (§2.4)。
//! 所有 handler 默认抹掉身份字段（model 层不接收），envelope 仅暴露查询/课业相关字段。

use anyhow::Result;

use crate::apps::jwc::{Client, GpaRank, GpaScope, LOGIN_URL};
use crate::auth::cas::with_cas_refresh;
use crate::output::{render, Envelope, OutputFormat};

use super::data::{ExamsData, GpaData, GradesData, ScheduleData};

/// `sjtu jwc grades [--xnm 2025] [--xqm 3] [--page 1] [--limit 50]`：N305005 成绩查询。
pub async fn cmd_grades(
    xnm: Option<String>,
    xqm: Option<String>,
    page: u32,
    limit: u32,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let xnm_q = xnm.clone();
    let xqm_q = xqm.clone();
    let env_resp = with_cas_refresh("jwc", LOGIN_URL, |session| {
        let xnm = xnm_q.clone();
        let xqm = xqm_q.clone();
        async move {
            let client = Client::from_session(session)?;
            client
                .grades(xnm.as_deref(), xqm.as_deref(), page, limit)
                .await
        }
    })
    .await?;

    let returned = env_resp.items.len();
    let data = GradesData {
        xnm,
        xqm,
        page,
        limit,
        total_result: env_resp.total_result,
        total_page: env_resp.total_page,
        returned,
        items: env_resp.items,
    };
    render(Envelope::ok(data), fmt)
}

/// `sjtu jwc schedule [--xnm 2025] [--xqm 3]`：N2151 个人课表（学年学期）。
pub async fn cmd_schedule(
    xnm: Option<String>,
    xqm: Option<String>,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let xnm_q = xnm.clone();
    let xqm_q = xqm.clone();
    let resp = with_cas_refresh("jwc", LOGIN_URL, |session| {
        let xnm = xnm_q.clone();
        let xqm = xqm_q.clone();
        async move {
            let client = Client::from_session(session)?;
            client.schedule(xnm.as_deref(), xqm.as_deref()).await
        }
    })
    .await?;

    let returned = resp.kb_list.len();
    let data = ScheduleData {
        xnm,
        xqm,
        returned,
        xqjmc_map: resp.xqjmc_map,
        items: resp.kb_list,
    };
    render(Envelope::ok(data), fmt)
}

/// `sjtu jwc gpa [--scope hxkc|qbkc] [--rank njzy|nj|bj] [--from 202503] [--to 202503]`。
///
/// **两阶段调用**（§2.3）：先触发统计计算，再拉结果；CLI 隐藏中间步骤，对外只看 items。
pub async fn cmd_gpa(
    scope: GpaScope,
    rank: GpaRank,
    qs_xnxq: Option<String>,
    zz_xnxq: Option<String>,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let qs = qs_xnxq.clone();
    let zz = zz_xnxq.clone();
    let mut env_resp = with_cas_refresh("jwc", LOGIN_URL, |session| {
        let qs = qs.clone();
        let zz = zz.clone();
        async move {
            let client = Client::from_session(session)?;
            client.gpa(scope, rank, qs.as_deref(), zz.as_deref()).await
        }
    })
    .await?;
    // 双轨：保留 server 给的 gpapm/xjfpm 字符串，client 端 fill_parsed 填 RankPair。
    for g in &mut env_resp.items {
        g.fill_parsed();
    }
    let returned = env_resp.items.len();
    let data = GpaData {
        scope: scope_str(scope),
        rank: rank_str(rank),
        qs_xnxq,
        zz_xnxq,
        total_result: env_resp.total_result,
        returned,
        items: env_resp.items,
    };
    render(Envelope::ok(data), fmt)
}

/// `sjtu jwc exams [--xnm 2025] [--xqm 3] [--page 1] [--limit 50]`：N358105 考试信息。
pub async fn cmd_exams(
    xnm: Option<String>,
    xqm: Option<String>,
    page: u32,
    limit: u32,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let xnm_q = xnm.clone();
    let xqm_q = xqm.clone();
    let env_resp = with_cas_refresh("jwc", LOGIN_URL, |session| {
        let xnm = xnm_q.clone();
        let xqm = xqm_q.clone();
        async move {
            let client = Client::from_session(session)?;
            client
                .exams(xnm.as_deref(), xqm.as_deref(), page, limit)
                .await
        }
    })
    .await?;

    let returned = env_resp.items.len();
    let data = ExamsData {
        xnm,
        xqm,
        page,
        limit,
        total_result: env_resp.total_result,
        total_page: env_resp.total_page,
        returned,
        items: env_resp.items,
    };
    render(Envelope::ok(data), fmt)
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
