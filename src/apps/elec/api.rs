//! 电费 Client：struct + connect + 4 个只读端点。
//!
//! 认证链路：S2 `cas::cas_login("elec", "https://elec.sjtu.edu.cn/")`，独立 sub-session
//! 文件 `elec.json`（**不与** my.sjtu / jwc 共享 cookie，§7.1）。

use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;
use reqwest::Client as HttpClient;

use super::http::{build_http_client, fetch_json, BASE};
use super::models::{Balance, DailyUsage, Envelope, MeInfo, Monthly};
use super::throttle::Throttle;
use crate::auth::cas::cas_login;
use crate::error::SjtuCliError;

/// CAS 跳转目标：电费首页；CAS 链落点期望 `elec.sjtu.edu.cn` 域。
pub(super) const LOGIN_URL: &str = "https://elec.sjtu.edu.cn/";

/// 电费 Client。
pub struct Client {
    http: HttpClient,
    throttle: Arc<Throttle>,
    /// CAS 返回的元数据，供上层 Envelope 展示。
    pub login: LoginMeta,
}

/// 登录元数据。
#[derive(Debug, Clone)]
pub struct LoginMeta {
    pub from_cache: bool,
    pub elapsed_ms: u128,
    pub final_url: String,
}

impl Client {
    /// CAS 跳转 → 构造注入 cookie 的 HTTP client。
    pub async fn connect() -> Result<Self> {
        let r = cas_login("elec", LOGIN_URL).await?;
        let http = build_http_client(&r.session)?;
        Ok(Self {
            http,
            throttle: Arc::new(Throttle::new()),
            login: LoginMeta {
                from_cache: r.from_cache,
                elapsed_ms: r.elapsed_ms,
                final_url: r.final_url,
            },
        })
    }

    /// `GET /api/me/info` —— 当前用户绑定房间 + 身份字段。
    pub async fn me_info(&self) -> Result<MeInfo> {
        let url = format!("{BASE}/api/me/info");
        let env: Envelope<MeInfo> =
            fetch_json(&self.http, &self.throttle, &url, "/me/info").await?;
        env.entities
            .into_iter()
            .next()
            .ok_or_else(|| SjtuCliError::UpstreamError("/api/me/info entities 为空".into()).into())
    }

    /// `GET /api/ws/sydl` —— 余额（4 个 Decimal 字段）。
    pub async fn balance(&self) -> Result<Balance> {
        let url = format!("{BASE}/api/ws/sydl");
        let env: Envelope<Balance> =
            fetch_json(&self.http, &self.throttle, &url, "/ws/sydl").await?;
        env.entities
            .into_iter()
            .next()
            .ok_or_else(|| SjtuCliError::UpstreamError("/api/ws/sydl entities 为空".into()).into())
    }

    /// `GET /api/rechage/ydl?year=&month=` —— 月度聚合。
    ///
    /// **服务端坑（§7.6）**：`total:0` 是误标，`entities` 永远 1 条带 `last`/`now`，
    /// CLI 直接读 `entities[0]` 即可，忽略 `total`。
    pub async fn monthly(&self, year: u16, month: u8) -> Result<Monthly> {
        let url = format!("{BASE}/api/rechage/ydl?year={year}&month={month}");
        let env: Envelope<Monthly> =
            fetch_json(&self.http, &self.throttle, &url, "/rechage/ydl").await?;
        env.entities.into_iter().next().ok_or_else(|| {
            SjtuCliError::UpstreamError("/api/rechage/ydl entities 为空".into()).into()
        })
    }

    /// `GET /api/rechage/ydlmx?begin=&end=` —— 日明细（包含两端）。
    pub async fn history(&self, begin: NaiveDate, end: NaiveDate) -> Result<Vec<DailyUsage>> {
        let url = format!(
            "{BASE}/api/rechage/ydlmx?begin={}&end={}",
            begin.format("%Y-%m-%d"),
            end.format("%Y-%m-%d")
        );
        let env: Envelope<DailyUsage> =
            fetch_json(&self.http, &self.throttle, &url, "/rechage/ydlmx").await?;
        Ok(env.entities)
    }
}
