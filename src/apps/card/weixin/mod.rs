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

/// 抓余额。L1 修复：主 session 直透传 weixin_follow（手卷 redirect），
/// 不再误用 with_cas_refresh 借 sub_session（那个机制是给 ASP CAS 路径用的、
/// 而 weixin path 直接靠 jaccount cookie 走 OAuth2，sub_session 内容不适用）。
pub async fn fetch_balance(main_session: &Session) -> Result<CardInfo> {
    let client = build_weixin_client(main_session)?;
    let (final_url, body) = weixin_follow(&client, BALANCE_URL).await?;
    detect_stale_or_unexpected(&final_url, &body, 200)?;
    parse_balance(&body)
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

/// 检测最终落地 URL 是否仍在 jaccount 域 —— 此时主 session 已失效。
///
/// **L1 配套语义改**：weixin path 用主 jaccount session 直透传，
/// 不存在 sub_session 概念；不再抛 `SubSessionStale("card_weixin")`
/// （那个信号是给 cas retry 层用的）。改抛 `SessionExpired`，
/// 提示用户跑 `sjtu login` 重新扫码。
fn detect_stale_or_unexpected(final_url: &str, body: &str, status: u16) -> Result<()> {
    if final_url.contains("jaccount.sjtu.edu.cn/jaccount/jalogin")
        || final_url.contains("jaccount.sjtu.edu.cn/oauth2/authorize")
    {
        return Err(SjtuCliError::SessionExpired.into());
    }
    if status != 200 {
        return Err(anyhow!("weixin 非 200 响应 status={status}"));
    }
    if !body.contains("<table") && !body.contains("<TABLE") {
        return Err(anyhow!("weixin 响应不含 <table>，可能 HTML 改版"));
    }
    Ok(())
}

/// 把 URL 里的裸空格替换为 `%20`。SJTU PHP OAuth2 endpoint 返回的 Location 头
/// `scope` 参数含多个空格分隔的 scope，未做 percent-encoding，违反 RFC 3986；
/// 浏览器宽容自动 fixup，但 reqwest 严格 URL parser 拒整条 Location 导致
/// redirect middleware short-circuit（D12 三层 bug L3）。本函数只处理空格，
/// 其它非法字符按 fail-fast 思路不动。
fn sanitize_location(loc: &str) -> String {
    loc.replace(' ', "%20")
}

/// 手卷 redirect chain：每跳 GET、读 Location、sanitize 空格、url.join、继续。
/// 直到非 3xx 响应或超过 15 跳。绕开 reqwest 严格 URL parser 对裸空格 Location 的拒绝。
async fn weixin_follow(client: &reqwest::Client, start: &str) -> Result<(String, String)> {
    use reqwest::header::LOCATION;
    let mut url = reqwest::Url::parse(start)
        .map_err(|e| SjtuCliError::NetworkError(format!("parse start url: {e}")))?;
    for hop in 0..15 {
        tracing::debug!(hop, %url, "weixin-follow GET");
        let resp = client
            .get(url.clone())
            .send()
            .await
            .map_err(|e| SjtuCliError::NetworkError(format!("GET hop {hop}: {e}")))?;
        if !resp.status().is_redirection() {
            let final_url = resp.url().to_string();
            let body = resp
                .text()
                .await
                .map_err(|e| SjtuCliError::NetworkError(format!("read body: {e}")))?;
            return Ok((final_url, body));
        }
        let loc_raw = resp
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| anyhow!("3xx hop {hop} 缺 Location"))?
            .to_string();
        let sanitized = sanitize_location(&loc_raw);
        url = url
            .join(&sanitized)
            .map_err(|e| anyhow!("hop {hop} Location join 失败：{e}"))?;
    }
    Err(anyhow!("weixin-follow 超过 15 跳"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn sanitize_location_replaces_spaces() {
        let input = "https://j.sjtu.edu.cn/oauth2/authorize?scope=profile card_info&state=4";
        let out = sanitize_location(input);
        assert_eq!(
            out,
            "https://j.sjtu.edu.cn/oauth2/authorize?scope=profile%20card_info&state=4"
        );
    }

    #[test]
    fn sanitize_location_no_change_when_already_encoded() {
        let input = "https://x/y?z=a%20b";
        assert_eq!(sanitize_location(input), input);
    }

    #[test]
    fn sanitize_location_handles_multiple_spaces() {
        assert_eq!(sanitize_location("a b c d"), "a%20b%20c%20d");
    }

    #[tokio::test]
    async fn weixin_follow_follows_chain_with_space_in_scope() {
        let mut server = mockito::Server::new_async().await;
        let host = server.host_with_port();
        let m1 = server
            .mock("GET", "/start")
            .with_status(302)
            .with_header(
                "Location",
                &format!("http://{host}/next?scope=a b&state=4"),
            )
            .create_async()
            .await;
        let m2 = server
            .mock("GET", "/next")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("scope".into(), "a b".into()),
                mockito::Matcher::UrlEncoded("state".into(), "4".into()),
            ]))
            .with_status(200)
            .with_body("<html>final</html>")
            .create_async()
            .await;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .unwrap();
        let (final_url, body) = weixin_follow(&client, &format!("http://{host}/start"))
            .await
            .unwrap();
        assert!(final_url.contains("/next"), "final_url={final_url}");
        assert_eq!(body, "<html>final</html>");
        m1.assert_async().await;
        m2.assert_async().await;
    }

    #[tokio::test]
    async fn weixin_follow_errors_after_15_hops() {
        let mut server = mockito::Server::new_async().await;
        let host = server.host_with_port();
        let _m = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(302)
            .with_header("Location", &format!("http://{host}/loop"))
            .expect_at_least(15)
            .create_async()
            .await;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .unwrap();
        let r = weixin_follow(&client, &format!("http://{host}/start")).await;
        assert!(r.is_err());
        assert!(format!("{:#}", r.unwrap_err()).contains("超过 15 跳"));
    }

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
    fn detect_session_expired_on_jalogin_redirect() {
        let url = "https://jaccount.sjtu.edu.cn/jaccount/jalogin?...";
        let r = detect_stale_or_unexpected(url, "<table></table>", 200);
        assert!(r.is_err());
        let err = r.unwrap_err();
        let downcast = err.downcast_ref::<SjtuCliError>();
        assert!(matches!(downcast, Some(SjtuCliError::SessionExpired)));
    }

    #[test]
    fn detect_session_expired_on_oauth_authorize_redirect() {
        let url = "https://jaccount.sjtu.edu.cn/oauth2/authorize?client_id=...";
        let r = detect_stale_or_unexpected(url, "<table></table>", 200);
        let err = r.unwrap_err();
        let downcast = err.downcast_ref::<SjtuCliError>();
        assert!(matches!(downcast, Some(SjtuCliError::SessionExpired)));
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
