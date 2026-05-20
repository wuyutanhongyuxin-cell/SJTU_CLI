//! 图书馆 Client：主 jaccount session 注入 + OAuth dance + 4 个只读端点。
//!
//! 鉴权链：
//! 1. 主 session → reqwest jar（`*.sjtu.edu.cn` HttpOnly 自动分发）
//! 2. `Client::connect` 首次跑 `GET /wechat/sjtu/nowlend` 触发服务端 OAuth dance
//!    → 跳 jaccount oAuthSJTU → 已登录用户透明回 weijieyue 兑 JSESSIONID
//!    → 落 jar，后续业务调用自动带
//! 3. 每次业务 XHR 前先 `getSessionId` 拿一次性 50 字符 token，
//!    随后调 `getInfo?session=<sid>` / `getHistoryBorrow?session=<sid>` / `getFineInfo?session=<sid>`
//!
//! Stale 信号：落地 URL 在 jaccount 域 → `SessionExpired`（http.rs::fetch_once 内置检测）。

use std::sync::Arc;

use anyhow::Result;
use reqwest::Client as HttpClient;

use super::http::{build_http_client, fetch_json, BASE};
use super::models::{
    Fine, FineInfoResp, GetInfoResp, HistoryBorrowResp, HistoryRow, Loan, PidResp, SessionIdResp,
};
use super::throttle::Throttle;
use crate::cookies::Session;
use crate::error::SjtuCliError;

/// OAuth dance 入口（主 session 已登录用户**透明**完成）。
pub(super) const OAUTH_URL: &str =
    "/wechat/sjtuAuth/oAuthSJTU?platform=phone&returnUrl=/sjtu/nowlend";

/// 健康检查端点。
pub(super) const PID_URL: &str = "/wechat/sjtuAuth/getPidFromSession";

/// 一次性 session token 端点。
pub(super) const SESSION_ID_URL: &str = "/wechat/sjtuAuth/getSessionId";

/// 当前借阅。
pub(super) const GET_INFO_URL: &str = "/wechat/sjtuAuth/getInfo";

/// 历史借阅。
pub(super) const HISTORY_URL: &str = "/wechat/sjtuAuth/getHistoryBorrow";

/// 罚款。
pub(super) const FINE_URL: &str = "/wechat/sjtuAuth/getFineInfo";

/// 图书馆 Client。
pub struct Client {
    http: HttpClient,
    throttle: Arc<Throttle>,
    pub login: LoginMeta,
}

/// 登录元数据（debug 用 + Envelope.meta 显示）。
#[derive(Debug, Clone)]
pub struct LoginMeta {
    /// OAuth dance 落地的最终 URL（验证不在 jaccount 域）。
    pub final_url: String,
    /// 健康检查 pid（脱敏，留前 8 位）。
    pub pid_redacted: Option<String>,
}

impl Client {
    /// 用主 jaccount session 构造 Client 并完成 OAuth dance。
    pub async fn connect(main_session: &Session) -> Result<Self> {
        let http = build_http_client(main_session)?;
        let throttle = Arc::new(Throttle::new());

        // OAuth dance：reqwest 自动 follow redirect 链；落地后服务端 JSESSIONID 在 jar 里。
        let dance_url = format!("{BASE}{OAUTH_URL}");
        let resp = http
            .get(&dance_url)
            .send()
            .await
            .map_err(|e| SjtuCliError::NetworkError(format!("OAuth dance: {e}")))?;
        let final_url = resp.url().to_string();
        if final_url.contains("jaccount.sjtu.edu.cn/jaccount/jalogin")
            || final_url.contains("jaccount.sjtu.edu.cn/oauth2/authorize")
        {
            return Err(SjtuCliError::SessionExpired.into());
        }
        // body 弃，dance 成功 = JSESSIONID 已落 jar。
        let _ = resp.text().await;

        // 健康检查：getPidFromSession 验证 dance 真的成功了。
        let pid_url = format!("{BASE}{PID_URL}");
        let pid: PidResp = fetch_json(&http, &throttle, &pid_url, "/getPidFromSession").await?;
        if pid.result != 1 {
            return Err(SjtuCliError::SubSystemUnreachable(
                "library",
                format!("getPidFromSession result={}", pid.result),
            )
            .into());
        }
        let pid_redacted = pid.data.as_ref().map(|s| {
            let prefix: String = s.chars().take(8).collect();
            format!("{prefix}***")
        });

        Ok(Self {
            http,
            throttle,
            login: LoginMeta {
                final_url,
                pid_redacted,
            },
        })
    }

    /// 拿一次性 50 字符 session token。每次业务调用前都要刷新。
    async fn get_session_id(&self) -> Result<String> {
        let url = format!("{BASE}{SESSION_ID_URL}");
        let r: SessionIdResp =
            fetch_json(&self.http, &self.throttle, &url, "/getSessionId").await?;
        if r.result != 1 {
            return Err(
                SjtuCliError::UpstreamError(format!("getSessionId result={}", r.result)).into(),
            );
        }
        r.data
            .ok_or_else(|| SjtuCliError::UpstreamError("getSessionId data 为空".into()).into())
    }

    /// `GET /sjtuAuth/getInfo?session=<sid>` —— 当前借阅。
    pub async fn loans(&self) -> Result<Vec<Loan>> {
        let sid = self.get_session_id().await?;
        let url = format!("{BASE}{GET_INFO_URL}?session={sid}");
        let r: GetInfoResp = fetch_json(&self.http, &self.throttle, &url, "/getInfo").await?;
        if r.result != 1 {
            return Err(SjtuCliError::UpstreamError(format!("getInfo result={}", r.result)).into());
        }
        Ok(r.borrow_array)
    }

    /// `GET /sjtuAuth/getHistoryBorrow?session=<sid>` —— 历史借阅。
    pub async fn history(&self) -> Result<Vec<HistoryRow>> {
        let sid = self.get_session_id().await?;
        let url = format!("{BASE}{HISTORY_URL}?session={sid}");
        let r: HistoryBorrowResp =
            fetch_json(&self.http, &self.throttle, &url, "/getHistoryBorrow").await?;
        if r.result != 1 {
            return Err(SjtuCliError::UpstreamError(format!(
                "getHistoryBorrow result={}",
                r.result
            ))
            .into());
        }
        Ok(r.history_array)
    }

    /// `GET /sjtuAuth/getFineInfo?session=<sid>` —— 罚款。
    pub async fn fines(&self) -> Result<Vec<Fine>> {
        let sid = self.get_session_id().await?;
        let url = format!("{BASE}{FINE_URL}?session={sid}");
        let r: FineInfoResp = fetch_json(&self.http, &self.throttle, &url, "/getFineInfo").await?;
        if r.result != 1 {
            return Err(
                SjtuCliError::UpstreamError(format!("getFineInfo result={}", r.result)).into(),
            );
        }
        Ok(r.fine_array)
    }
}
