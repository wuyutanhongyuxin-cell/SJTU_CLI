//! POST `jaccount.sjtu.edu.cn/oauth2/token`：authorization_code 换 token / refresh_token 续期。
//!
//! 服务端响应（200 OK）：
//! ```json
//! {"expires_in":1800,"token_type":"Bearer","refresh_token":"...","access_token":"..."}
//! ```
//!
//! 错误响应（400 / 401）：
//! ```json
//! {"error":"invalid_grant","error_description":"..."}
//! ```

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::error::SjtuCliError;

const TOKEN_URL: &str = "https://jaccount.sjtu.edu.cn/oauth2/token";

/// 服务端返回的 token 响应。
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    /// 有效期秒数，文档 1800（30 分钟）。
    pub expires_in: u64,
    pub token_type: String,
}

/// 用 `authorization_code` 换 token。
pub async fn exchange_code(
    client: &reqwest::Client,
    code: &str,
    redirect_uri: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<TokenResponse> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("client_secret", client_secret),
    ];
    post_token(client, &params, "exchange_code").await
}

/// 用 `refresh_token` 续期。
pub async fn refresh(
    client: &reqwest::Client,
    refresh_token: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<TokenResponse> {
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
        ("client_secret", client_secret),
    ];
    post_token(client, &params, "refresh").await
}

/// POST /oauth2/token 公共部分。
async fn post_token(
    client: &reqwest::Client,
    params: &[(&str, &str)],
    label: &str,
) -> Result<TokenResponse> {
    post_token_to(client, TOKEN_URL, params, label).await
}

/// 同 `post_token` 但允许覆盖 URL（测试用 mockito server URL）。
pub(crate) async fn post_token_to(
    client: &reqwest::Client,
    url: &str,
    params: &[(&str, &str)],
    label: &str,
) -> Result<TokenResponse> {
    let resp = client
        .post(url)
        .form(params)
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("POST {url} ({label}): {e}")))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .with_context(|| format!("读 {url} body 失败"))?;
    if !status.is_success() {
        return Err(SjtuCliError::CardOAuth(format!(
            "{label} status={status} body={}",
            truncate(&body, 200)
        ))
        .into());
    }
    serde_json::from_str::<TokenResponse>(&body).map_err(|e| {
        SjtuCliError::CardOAuth(format!(
            "{label} 解析 token JSON 失败: {e}, snippet={}",
            truncate(&body, 200)
        ))
        .into()
    })
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

