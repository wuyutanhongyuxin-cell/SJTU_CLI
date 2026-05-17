//! 一卡通 API Client：连接 OAuth2 session → 调 `/v1/me/card*` 端点。
//!
//! 鉴权：spec §6.1，要求 caller 已持有 fresh access_token（通过 oauth2_dev::load_session）。
//! token 过期识别：see http::detect_token_expired_in_body。
//! refresh 责任在调用方（commands/card/handlers.rs）用 with_token_refresh 包裹。

use anyhow::Result;
use chrono::NaiveDate;

use super::http::{build_http_client, detect_token_expired_in_body, fetch_json_raw, BASE};
use super::models::{CardInfo, Envelope, Transaction};
use super::throttle::Throttle;
use crate::auth::oauth2_dev::{self, CardOAuthSession};
use crate::error::SjtuCliError;

/// 一卡通 client。每个命令 connect 一次；不长持有。
pub struct Client {
    http: reqwest::Client,
    throttle: Throttle,
    session: CardOAuthSession,
}

impl Client {
    /// 从本地 card_oauth.json 读 session 构造 client。
    /// session 未登录 → `NotAuthenticated`；fresh 检查在 handler 层做。
    pub async fn connect() -> Result<Self> {
        let session = oauth2_dev::load_session()?;
        let http = build_http_client()?;
        Ok(Self {
            http,
            throttle: Throttle::new(),
            session,
        })
    }

    /// 暴露 access_token 给 caller（仅供同 crate fetch 路径用）。
    pub(crate) fn access_token(&self) -> &str {
        &self.session.access_token
    }

    /// session 持有的主卡号（若已存）。
    pub fn main_card_no(&self) -> Option<&str> {
        self.session.main_card_no.as_deref()
    }

    /// `GET /v1/me/card`：拿当前用户的卡信息（多张卡时 entities 多条）。
    /// 默认取 `entities[0]` 作"主卡"。
    pub async fn get_balance(&self) -> Result<CardInfo> {
        let url = format!("{BASE}/v1/me/card");
        let body = fetch_json_raw(
            &self.http,
            &self.throttle,
            &url,
            self.access_token(),
            "card_info",
        )
        .await?;
        if let Some(e) = detect_token_expired_in_body(&body) {
            return Err(e);
        }
        let env: Envelope<CardInfo> = serde_json::from_str(&body).map_err(|e| {
            SjtuCliError::UpstreamError(format!(
                "card_info JSON 解析失败: {e}, snippet={}",
                snip(&body)
            ))
        })?;
        env.entities
            .into_iter()
            .next()
            .ok_or_else(|| SjtuCliError::UpstreamError("card_info entities 为空".into()).into())
    }

    /// `GET /v1/me/card/transactions`：拿时间窗口内的消费记录。
    /// `card_no` 必传（避免多卡用户的不确定性）；`limit` clamp 到 1..=100。
    pub async fn get_transactions(
        &self,
        card_no: &str,
        begin: NaiveDate,
        end: NaiveDate,
        limit: u32,
    ) -> Result<(u64, Vec<Transaction>)> {
        let begin_ms = begin
            .and_hms_opt(0, 0, 0)
            .expect("00:00:00 valid")
            .and_utc()
            .timestamp_millis();
        let end_ms = end
            .and_hms_opt(23, 59, 59)
            .expect("23:59:59 valid")
            .and_utc()
            .timestamp_millis();
        let clamped_limit = limit.clamp(1, 100);
        let url = format!(
            "{BASE}/v1/me/card/transactions?cardNo={card_no}&beginDate={begin_ms}&endDate={end_ms}&orderBy=dateTime&start=0&limit={clamped_limit}"
        );
        let body = fetch_json_raw(
            &self.http,
            &self.throttle,
            &url,
            self.access_token(),
            "card_transactions",
        )
        .await?;
        if let Some(e) = detect_token_expired_in_body(&body) {
            return Err(e);
        }
        let env: Envelope<Transaction> = serde_json::from_str(&body).map_err(|e| {
            SjtuCliError::UpstreamError(format!(
                "card_transactions JSON 解析失败: {e}, snippet={}",
                snip(&body)
            ))
        })?;
        let total = env.total.unwrap_or(env.entities.len() as u64);
        Ok((total, env.entities))
    }
}

/// 错误 snippet 截断：char-aware，避免 UTF-8 多字节边界 panic（同 T10 followup 修复）。
fn snip(s: &str) -> String {
    let max = 200;
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push_str("...");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // 注：Client::connect 依赖磁盘上的 card_oauth.json，单测无法做真机。
    // 单测覆盖 get_balance / get_transactions 的 happy path 用 mockito 模拟整个 BASE。
    //
    // 但 BASE 是常量；要在测试里覆盖需要把 url 注入。这里偷工：单测只验证
    // helper 数学 + snip UTF-8 安全。完整 e2e 留 CP-T4-BAL/HIST。

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn date_to_ms_begin_of_day() {
        let d = ymd(2026, 5, 17);
        let ms = d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis();
        // 2026-05-17 00:00:00 UTC
        assert_eq!(ms, 1778976000000);
    }

    #[test]
    fn date_to_ms_end_of_day() {
        let d = ymd(2026, 5, 17);
        let ms = d.and_hms_opt(23, 59, 59).unwrap().and_utc().timestamp_millis();
        // 2026-05-17 23:59:59 UTC = begin_of_day + 86399s
        assert_eq!(ms, 1778976000000 + 23 * 3600 * 1000 + 59 * 60 * 1000 + 59 * 1000);
    }

    #[test]
    fn snip_truncates_utf8_safe() {
        let s = "你好世界".repeat(60);
        let t = snip(&s);
        assert!(t.ends_with("..."));
        assert_eq!(t.chars().count(), 200 + 3);
    }
}
