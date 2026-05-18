//! via-path 实现：OAuth2 path 与 weixin path 的内层 async 函数。
//! 由 `handlers.rs` 中的 `cmd_balance` / `cmd_history` dispatch 调用。

use std::time::Instant;

use anyhow::Result;
use chrono::{Duration, FixedOffset, TimeZone};
use rust_decimal::Decimal;

use super::data::{
    redact_bank_no, redact_card_no, BalanceData, HistoryData, TransactionItem, UserIdentity,
};
use super::refresh_helper::ensure_fresh_and_call;
use crate::apps::card::weixin;
use crate::auth::oauth2_dev;
use crate::cookies;
use crate::error::SjtuCliError;
use crate::output::{render, Envelope, EnvelopeMeta, OutputFormat};

/// 加载主 jaccount session。文件不存在 → NotAuthenticated（提示用户 `sjtu login`）。
pub(super) fn load_main_session() -> Result<cookies::Session> {
    cookies::load_session()
}

/// OAuth2 path balance：原 cmd_balance 主体不变，render 换 ok_with_meta。
pub(super) async fn cmd_balance_oauth2(
    with_identity: bool,
    meta: EnvelopeMeta,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let started = Instant::now();
    let info = ensure_fresh_and_call(|c| async move { c.get_balance().await }).await?;
    let user = if with_identity {
        info.user.as_ref().map(|u| UserIdentity {
            code: u.code.clone().unwrap_or_default(),
            name: u.name.clone().unwrap_or_default(),
            organize: u
                .organize
                .as_ref()
                .and_then(|o| o.name.clone())
                .unwrap_or_default(),
        })
    } else {
        None
    };
    let bank_no_redacted = if with_identity {
        info.bank_no.as_deref().map(redact_bank_no)
    } else {
        None
    };
    let face_sub_type = if with_identity {
        info.face_sub_type.clone()
    } else {
        None
    };
    let data = BalanceData {
        card_no_redacted: redact_card_no(&info.card_no),
        balance: info.card_balance,
        trans_balance: info.trans_balance,
        expire_date: info.expire_date.clone(),
        lost: info.lost,
        frozen: info.frozen,
        face_type: info.face_type.clone(),
        face_sub_type,
        user,
        bank_no_redacted,
        from_cache: false,
        elapsed_ms: started.elapsed().as_millis(),
    };
    render(Envelope::ok_with_meta(data, meta), fmt)
}

/// weixin path balance：主 jaccount session 透明跳抓 HTML，PII 红线不出 identity。
pub(super) async fn cmd_balance_weixin(
    with_identity: bool,
    meta: EnvelopeMeta,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    if with_identity {
        tracing::warn!("weixin path 不支持 --with-identity（PII 红线）；该 flag 已忽略");
    }
    let started = Instant::now();
    let main_session = load_main_session()?;
    let info = weixin::fetch_balance(&main_session).await?;
    let data = BalanceData::from_weixin_card_info(&info, started.elapsed().as_millis());
    render(Envelope::ok_with_meta(data, meta), fmt)
}

/// OAuth2 path history：原 cmd_history 主体不变，render 换 ok_with_meta。
pub(super) async fn cmd_history_oauth2(
    days: u32,
    limit: u32,
    meta: EnvelopeMeta,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let started = Instant::now();
    let end_local = chrono::Local::now().date_naive();
    let begin_local = end_local - Duration::days((days as i64) - 1);

    let sess = oauth2_dev::load_session()?;
    let card_no = sess.main_card_no.clone().ok_or_else(|| {
        SjtuCliError::CardOAuth(
            "session 缺 main_card_no；先跑 `sjtu card balance` 一次以初始化".into(),
        )
    })?;
    let card_no_for_call = card_no.clone();
    let begin = begin_local;
    let end = end_local;
    let (total, txs) = ensure_fresh_and_call(move |c| {
        let card_no = card_no_for_call.clone();
        async move { c.get_transactions(&card_no, begin, end, limit).await }
    })
    .await?;

    let beijing = FixedOffset::east_opt(8 * 3600).expect("+08:00");
    let items: Vec<TransactionItem> = txs
        .into_iter()
        .map(|t| TransactionItem {
            consumed_at: beijing
                .timestamp_millis_opt(t.date_time_ms)
                .single()
                .unwrap_or_else(|| {
                    tracing::warn!(
                        "card_history: 无效 date_time_ms={} fallback to epoch（merchant={:?}）",
                        t.date_time_ms,
                        t.merchant
                    );
                    beijing.timestamp_millis_opt(0).unwrap()
                }),
            system: t.system,
            merchant_no: t.merchant_no,
            merchant: t.merchant,
            description: t.description,
            amount: t.amount,
            balance_after: t.card_balance,
        })
        .collect();
    let total_amount: Decimal = items.iter().map(|t| t.amount).sum();
    let data = HistoryData {
        card_no_redacted: redact_card_no(&card_no),
        begin_date_local: begin_local.format("%Y-%m-%d").to_string(),
        end_date_local: end_local.format("%Y-%m-%d").to_string(),
        returned: items.len(),
        total,
        transactions: items,
        total_amount,
        from_cache: false,
        elapsed_ms: started.elapsed().as_millis(),
    };
    render(Envelope::ok_with_meta(data, meta), fmt)
}

/// weixin path history：主 jaccount session 抓 HTML，PII 无卡号（占位 `<weixin>`）。
pub(super) async fn cmd_history_weixin(
    days: u32,
    limit: u32,
    meta: EnvelopeMeta,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let started = Instant::now();
    let end_local = chrono::Local::now().date_naive();
    let begin_local = end_local - Duration::days((days as i64) - 1);
    let main_session = load_main_session()?;
    let mut txs = weixin::fetch_history(&main_session, Some(begin_local), Some(end_local)).await?;
    if txs.len() > limit as usize {
        txs.truncate(limit as usize);
    }
    let data = HistoryData::from_weixin_transactions(
        &txs,
        begin_local,
        end_local,
        started.elapsed().as_millis(),
    );
    render(Envelope::ok_with_meta(data, meta), fmt)
}
