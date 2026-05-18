//! `sjtu card <sub>` pub 入口：cmd_auth / cmd_balance / cmd_history。
//! 具体 path 实现（OAuth2 / weixin）在 `handlers_dispatch.rs`。

use anyhow::Result;

use super::data::redact_card_no;
use super::handlers_dispatch::{
    cmd_balance_oauth2, cmd_balance_weixin, cmd_history_oauth2, cmd_history_weixin,
};
use crate::apps::card::via::{select_via, CardVia, ResolvedVia};
use crate::auth::oauth2_dev::{self, authorize, callback, secret, token, CardOAuthSession};
use crate::error::SjtuCliError;
use crate::output::{render, Envelope, EnvelopeMeta, OutputFormat};

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

    let client = crate::apps::card::Client::connect().await?;
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

/// `sjtu card balance [--with-identity] [--via auto|oauth2|weixin]`：当前卡余额查询。
pub async fn cmd_balance(
    with_identity: bool,
    via: CardVia,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    let has_oauth_token = oauth2_dev::load_session().is_ok();
    let resolved = select_via(via, has_oauth_token);
    let meta = EnvelopeMeta {
        via: Some(resolved.name().to_string()),
        source_hint: Some(resolved.source_hint().to_string()),
    };
    match resolved {
        ResolvedVia::Weixin => cmd_balance_weixin(with_identity, meta, fmt).await,
        ResolvedVia::Oauth2 => cmd_balance_oauth2(with_identity, meta, fmt).await,
    }
}

/// `sjtu card history --days N --limit M [--via auto|oauth2|weixin]`：消费记录查询。
pub async fn cmd_history(
    days: u32,
    limit: u32,
    via: CardVia,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    if days == 0 || days > 365 {
        return Err(SjtuCliError::InvalidInput(format!("--days {days} 超出范围 (1..=365)")).into());
    }
    let has_oauth_token = oauth2_dev::load_session().is_ok();
    let resolved = select_via(via, has_oauth_token);
    let meta = EnvelopeMeta {
        via: Some(resolved.name().to_string()),
        source_hint: Some(resolved.source_hint().to_string()),
    };
    match resolved {
        ResolvedVia::Weixin => cmd_history_weixin(days, limit, meta, fmt).await,
        ResolvedVia::Oauth2 => cmd_history_oauth2(days, limit, meta, fmt).await,
    }
}
