//! weixin path 入口：通过主 jaccount session cookie 透明跳 OAuth2，HTML scrape ecard*.php。
//!
//! 入口形态：
//! ```ignore
//! let info = fetch_balance(&main_session).await?;        // CardInfo
//! let txs  = fetch_history(&main_session, None, None).await?;  // Vec<Transaction>
//! ```
//!
//! 鉴权与 stale-detect：复用 `crate::auth::cas::retry::with_cas_refresh`（T8）。stale variant
//! `SubSessionStale("card_weixin")` 由 redirect 链落在 jaccount jalogin 时由本模块手动抛。

pub mod balance_parse;
pub mod client;
pub mod history_parse;
pub mod money;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;

use self::balance_parse::parse_balance;
use self::client::build_weixin_client;
use self::history_parse::{parse_history, parse_history_summary, HistorySummary};
use crate::apps::card::models::{CardInfo, Transaction};
use crate::auth::cas::retry::with_cas_refresh;
use crate::cookies::Session;
use crate::error::SjtuCliError;

const BALANCE_URL: &str = "https://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecardbalance.php";
const HISTORY_URL: &str = "https://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecardbill.php";

/// 抓余额。with_cas_refresh 包装，stale 时自动重 cas + 重抓。
pub async fn fetch_balance(_main_session: &Session) -> Result<CardInfo> {
    with_cas_refresh("card_weixin", BALANCE_URL, |session| async move {
        let client = build_weixin_client(&session)?;
        let resp = client
            .get(BALANCE_URL)
            .send()
            .await
            .map_err(|e| SjtuCliError::NetworkError(format!("GET balance: {e}")))?;
        let status = resp.status();
        let final_url = resp.url().to_string();
        let body = resp
            .text()
            .await
            .map_err(|e| SjtuCliError::NetworkError(format!("读 balance body: {e}")))?;
        detect_stale_or_unexpected(&final_url, &body, status.as_u16())?;
        parse_balance(&body)
    })
    .await
}

/// 抓消费记录。`start`/`end` 是日期窗口（默认服务端最近 30 天，OQ-WX-1 plan 阶段未确定参数名）。
pub async fn fetch_history(
    _main_session: &Session,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
) -> Result<Vec<Transaction>> {
    let url = build_history_url(start, end);
    let url_clone = url.clone();
    with_cas_refresh("card_weixin", &url_clone, |session| {
        let url = url.clone();
        async move {
            let client = build_weixin_client(&session)?;
            let resp = client
                .get(&url)
                .send()
                .await
                .map_err(|e| SjtuCliError::NetworkError(format!("GET history: {e}")))?;
            let status = resp.status();
            let final_url = resp.url().to_string();
            let body = resp
                .text()
                .await
                .map_err(|e| SjtuCliError::NetworkError(format!("读 history body: {e}")))?;
            detect_stale_or_unexpected(&final_url, &body, status.as_u16())?;
            parse_history(&body)
        }
    })
    .await
}

/// 同步抓 footer 汇总（CLI 暂不出，留作 history 命令未来扩展）。
pub async fn fetch_history_summary(
    _main_session: &Session,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
) -> Result<HistorySummary> {
    let url = build_history_url(start, end);
    let url_clone = url.clone();
    with_cas_refresh("card_weixin", &url_clone, |session| {
        let url = url.clone();
        async move {
            let client = build_weixin_client(&session)?;
            let resp = client
                .get(&url)
                .send()
                .await
                .map_err(|e| SjtuCliError::NetworkError(format!("GET history summary: {e}")))?;
            let body = resp
                .text()
                .await
                .map_err(|e| SjtuCliError::NetworkError(format!("读 body: {e}")))?;
            Ok(parse_history_summary(&body))
        }
    })
    .await
}

/// OQ-WX-1 plan 阶段假定 query 参数名 `startdate` / `enddate`（CP 阶段实测后调整）。
fn build_history_url(start: Option<NaiveDate>, end: Option<NaiveDate>) -> String {
    match (start, end) {
        (Some(s), Some(e)) => format!("{HISTORY_URL}?startdate={s}&enddate={e}"),
        (Some(s), None) => format!("{HISTORY_URL}?startdate={s}"),
        (None, Some(e)) => format!("{HISTORY_URL}?enddate={e}"),
        (None, None) => HISTORY_URL.to_string(),
    }
}

/// 检测 redirect 链是否被 jaccount 拦截（stale 形态 OQ-WX-2 plan 假定）。
/// 命中 → 抛 `SubSessionStale("card_weixin")`，由 `with_cas_refresh` 接住重试。
fn detect_stale_or_unexpected(final_url: &str, body: &str, status: u16) -> Result<()> {
    if final_url.contains("jaccount.sjtu.edu.cn/jaccount/jalogin")
        || final_url.contains("jaccount.sjtu.edu.cn/oauth2/authorize")
    {
        return Err(SjtuCliError::SubSessionStale("card_weixin").into());
    }
    if status != 200 {
        return Err(anyhow!("weixin 非 200 响应 status={status}"));
    }
    if !body.contains("<table") && !body.contains("<TABLE") {
        return Err(anyhow!("weixin 响应不含 <table>，可能 HTML 改版"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn build_history_url_with_both_dates() {
        let s = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let e = NaiveDate::from_ymd_opt(2026, 5, 31).unwrap();
        assert_eq!(
            build_history_url(Some(s), Some(e)),
            format!("{HISTORY_URL}?startdate=2026-05-01&enddate=2026-05-31")
        );
    }

    #[test]
    fn build_history_url_with_no_dates() {
        assert_eq!(build_history_url(None, None), HISTORY_URL);
    }

    #[test]
    fn detect_stale_on_jalogin_redirect() {
        let url = "https://jaccount.sjtu.edu.cn/jaccount/jalogin?...";
        let r = detect_stale_or_unexpected(url, "<table></table>", 200);
        assert!(r.is_err());
        let err = r.unwrap_err();
        let downcast = err.downcast_ref::<SjtuCliError>();
        assert!(matches!(
            downcast,
            Some(SjtuCliError::SubSessionStale("card_weixin"))
        ));
    }

    #[test]
    fn detect_stale_on_oauth_authorize_redirect() {
        let url = "https://jaccount.sjtu.edu.cn/oauth2/authorize?client_id=...";
        let r = detect_stale_or_unexpected(url, "<table></table>", 200);
        let err = r.unwrap_err();
        let downcast = err.downcast_ref::<SjtuCliError>();
        assert!(matches!(
            downcast,
            Some(SjtuCliError::SubSessionStale("card_weixin"))
        ));
    }

    #[test]
    fn detect_ok_on_real_response() {
        let url = "https://weixin.sjtu.edu.cn/xxzx/sjtu-net/ecard/ecardbalance.php";
        assert!(detect_stale_or_unexpected(url, "<html><table>...</table></html>", 200).is_ok());
    }

    #[test]
    fn detect_errors_on_non_200() {
        let url = "https://weixin.sjtu.edu.cn/.../ecardbalance.php";
        assert!(detect_stale_or_unexpected(url, "<table></table>", 500).is_err());
    }

    #[test]
    fn detect_errors_on_html_without_table() {
        let url = "https://weixin.sjtu.edu.cn/.../ecardbalance.php";
        let r = detect_stale_or_unexpected(url, "<html>nothing</html>", 200);
        assert!(r.is_err());
    }
}
