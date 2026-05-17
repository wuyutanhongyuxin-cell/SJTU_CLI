//! `sjtu card <sub>` 主流程：connect OAuth2 → API 调用（with_token_refresh 包裹）→ 渲染 Envelope。
//!
//! `cmd_auth`：手动触发 authorize → callback → token exchange → 落盘。
//! `cmd_balance`：默认抹身份；`--with-identity` 出 user / bank_no。
//! `cmd_history`：时间窗口 N 天（默认 30，最大 365）；limit 默认 50 最大 100。

use std::time::Instant;

use anyhow::Result;
use chrono::{Duration, FixedOffset, TimeZone};
use rust_decimal::Decimal;

use super::data::{
    redact_bank_no, redact_card_no, BalanceData, HistoryData, TransactionItem, UserIdentity,
};
use super::refresh_helper::ensure_fresh_and_call;
use crate::apps::card::Client;
use crate::auth::oauth2_dev::{self, authorize, callback, secret, token, CardOAuthSession};
use crate::error::SjtuCliError;
use crate::output::{render, Envelope, OutputFormat};

/// `sjtu card auth`：手动触发 OAuth2 授权流（首次使用时跑）。
pub async fn cmd_auth(client_id: String, fmt: Option<OutputFormat>) -> Result<()> {
    let secret = secret::load_secret()?;
    let state = authorize::generate_state();
    let url = authorize::build_authorize_url(
        &client_id,
        authorize::DEFAULT_REDIRECT_URI,
        authorize::DEFAULT_SCOPE,
        &state,
    )?;
    tracing::info!("OAuth2 authorize: 已构造 URL，启 listener 后打开浏览器");

    authorize::open_in_browser(&url).await?;

    let (code, got_state) = callback::wait_for_callback().await?;
    callback::check_state(&got_state, &state)?;
    tracing::info!("callback 拿到 code，开始换 token");

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 http: {e}")))?;
    let resp = token::exchange_code(
        &http,
        &code,
        authorize::DEFAULT_REDIRECT_URI,
        &client_id,
        &secret,
    )
    .await?;

    // compute_expires_at 只接受 expires_in: u64，内部自动用 beijing_now() 计算绝对时间
    let expires_in = resp.expires_in;
    let sess = CardOAuthSession {
        client_id,
        access_token: resp.access_token,
        refresh_token: resp.refresh_token,
        expires_at: oauth2_dev::compute_expires_at(expires_in),
        scope: authorize::DEFAULT_SCOPE.to_string(),
        main_card_no: None,
        captured_at: oauth2_dev::beijing_now(),
    };
    oauth2_dev::save_session(&sess)?;
    tracing::info!("OAuth2 session 已落盘 ~/.sjtu-cli/sub_sessions/card_oauth.json");

    let client = Client::connect().await?;
    let info = client.get_balance().await?;
    let mut updated = oauth2_dev::load_session()?;
    updated.main_card_no = Some(info.card_no.clone());
    oauth2_dev::save_session(&updated)?;

    render(
        Envelope::ok(serde_json::json!({
            "ok": true,
            "card_no_redacted": redact_card_no(&info.card_no),
            "expires_in_secs": expires_in,
            "scope": updated.scope,
        })),
        fmt,
    )
}

/// `sjtu card balance [--with-identity]`：当前卡余额查询。
pub async fn cmd_balance(with_identity: bool, fmt: Option<OutputFormat>) -> Result<()> {
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
    render(Envelope::ok(data), fmt)
}

/// `sjtu card history --days N --limit M`：消费记录查询。
pub async fn cmd_history(days: u32, limit: u32, fmt: Option<OutputFormat>) -> Result<()> {
    if days == 0 || days > 365 {
        return Err(SjtuCliError::InvalidInput(format!("--days {days} 超出范围 (1..=365)")).into());
    }
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
    render(Envelope::ok(data), fmt)
}
