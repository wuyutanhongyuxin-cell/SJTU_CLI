//! MailClient：SSO 跟链拿 ZM_AUTH_TOKEN + 4 个只读 SOAP 调用。

use std::sync::Arc;

use anyhow::Result;
use reqwest::Client as HttpClient;

use super::csrf::fetch_csrf_token;
use super::http::{build_http_client, extract_zm_auth_token, send_soap_request, BASE};
use super::models::{Mail, MailFull};
use super::parser::{parse_get_msg_response, parse_search_response};
use super::soap::{build_get_folder_envelope, build_get_msg_envelope, build_search_envelope};
use super::throttle::Throttle;
use crate::cookies::Session;
use crate::error::SjtuCliError;

/// 邮件 Client。
pub struct MailClient {
    http: HttpClient,
    throttle: Arc<Throttle>,
    auth_token: String,
    /// Zimbra CSRF token（hop 6 HTML 抽）。envelope `<csrfToken>` 必填。
    csrf_token: String,
    pub login: LoginMeta,
}

#[derive(Debug, Clone)]
pub struct LoginMeta {
    /// SSO 跟链落地 URL（验证不在 jaccount 域）。
    pub final_url: String,
    /// ZM_AUTH_TOKEN 脱敏（前 8 字符 + ***）。
    pub auth_token_redacted: String,
}

impl MailClient {
    /// 用主 jaccount session 构造 Client，SSO 跟链拿 ZM_AUTH_TOKEN。
    pub async fn connect(main_session: &Session) -> Result<Self> {
        let (http, jar) = build_http_client(main_session)?;
        let throttle = Arc::new(Throttle::new());

        // SSO 跟链：reqwest 自动 follow redirect 到 zimbra。
        let resp = http
            .get(format!("{BASE}/"))
            .send()
            .await
            .map_err(|e| SjtuCliError::NetworkError(format!("SSO 跟链: {e}")))?;
        let final_url = resp.url().to_string();
        if final_url.contains("jaccount.sjtu.edu.cn/jaccount/jalogin")
            || final_url.contains("jaccount.sjtu.edu.cn/oauth2/authorize")
        {
            return Err(SjtuCliError::SessionExpired.into());
        }
        // body 弃，cookie jar 已装填。
        let _ = resp.text().await;

        let auth_token = extract_zm_auth_token(&jar).ok_or_else(|| {
            SjtuCliError::SubSystemUnreachable(
                "mail",
                "ZM_AUTH_TOKEN cookie 未出现在 jar (SSO 跟链失败？)".to_string(),
            )
        })?;

        // Zimbra 强制 CSRF：ZM_AUTH_TOKEN payload 含 csrf=1:1 时 envelope 必填
        // <csrfToken>。token 嵌在 /zimbra/mail HTML 的 window.csrfToken。
        let csrf_token = fetch_csrf_token(&http).await?;

        let prefix: String = auth_token.chars().take(8).collect();
        let auth_token_redacted = format!("{prefix}***");

        Ok(Self {
            http,
            throttle,
            auth_token,
            csrf_token,
            login: LoginMeta {
                final_url,
                auth_token_redacted,
            },
        })
    }

    /// 通用 search：query 自由组合 `in:inbox` / `is:unread` / 关键字。
    pub async fn search(&self, query: &str, limit: u32, offset: u32) -> Result<Vec<Mail>> {
        let envelope =
            build_search_envelope(&self.auth_token, &self.csrf_token, query, limit, offset);
        let xml =
            send_soap_request(&self.http, &self.throttle, &envelope, "/SearchRequest").await?;
        parse_search_response(&xml)
    }

    /// `mails` 命令：inbox 列表。
    pub async fn inbox(&self, limit: u32, offset: u32) -> Result<Vec<Mail>> {
        self.search("in:inbox", limit, offset).await
    }

    /// `mails --unread` 命令：只未读。
    pub async fn unread(&self, limit: u32, offset: u32) -> Result<Vec<Mail>> {
        self.search("is:unread", limit, offset).await
    }

    /// `mails --search` 命令：关键字搜索（自动加 in:inbox 限定域，避免命中已发送）。
    pub async fn keyword(&self, kw: &str, limit: u32, offset: u32) -> Result<Vec<Mail>> {
        // Zimbra query 语法：双引号包字面字符串
        let q = format!(r#"in:inbox "{kw}""#);
        self.search(&q, limit, offset).await
    }

    /// `mail <uid>` 命令：读单封邮件正文。**强制 read=0**（envelope builder 已注入）。
    pub async fn get_msg(&self, msg_id: &str) -> Result<MailFull> {
        let envelope = build_get_msg_envelope(&self.auth_token, &self.csrf_token, msg_id);
        let xml =
            send_soap_request(&self.http, &self.throttle, &envelope, "/GetMsgRequest").await?;
        parse_get_msg_response(&xml)
    }

    /// 调试 / 验证用：拿 folder 树。CLI 暂不直接暴露，但 schema 探索时有用。
    #[allow(dead_code)]
    pub async fn folders(&self) -> Result<String> {
        let envelope = build_get_folder_envelope(&self.auth_token, &self.csrf_token);
        send_soap_request(&self.http, &self.throttle, &envelope, "/GetFolderRequest").await
    }
}
