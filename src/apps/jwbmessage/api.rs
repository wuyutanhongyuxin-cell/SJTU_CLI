//! 交我办消息 Client：struct + connect + 读端点 + 写端点转发。
//!
//! 认证链路：直接复用 S2 `cas::cas_login("jwbmessage", "https://my.sjtu.edu.cn/ui/app/")`。
//! 主 session 带 JAAuthCookie → 302 链直达 `my.sjtu.edu.cn/ui/app/` → 收 JSESSIONID 等 cookie。
//!
//! 写端点实现拆在 `api_write.rs`，读端点在本文件。

use std::sync::Arc;

use anyhow::Result;
use reqwest::Client as HttpClient;

use super::api_write;
use super::http::{build_http_client, fetch_json, BASE};
use super::models::{
    Group, GroupEnvelope, Message, MessageListEnvelope, ReadAllResponse, UnreadNum,
};
use super::throttle::Throttle;
use crate::auth::cas::cas_login;

/// CAS 跳转的目标：交我办门户首页，未登录时 JAAuthCookie 带走即回跳。
pub(super) const LOGIN_URL: &str = "https://my.sjtu.edu.cn/ui/app/";

/// 交我办 Client。字段 `pub(super)` 暴露给 `api_write`。
pub struct Client {
    pub(super) http: HttpClient,
    pub(super) throttle: Arc<Throttle>,
    /// CAS 返回的元数据，供上层 Envelope 展示。
    pub login: LoginMeta,
}

/// 登录元数据，暴露给 CLI 构造 Envelope。
#[derive(Debug, Clone)]
pub struct LoginMeta {
    pub from_cache: bool,
    pub elapsed_ms: u128,
    pub final_url: String,
}

impl Client {
    /// CAS 跳转 → 构造注入 cookie 的 HTTP client。
    pub async fn connect() -> Result<Self> {
        let r = cas_login("jwbmessage", LOGIN_URL).await?;
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

    /// GET /api/jwbmessage/unreadNum — 全局未读总数。
    pub async fn unread_num(&self) -> Result<UnreadNum> {
        let url = format!("{BASE}/api/jwbmessage/unreadNum");
        fetch_json(&self.http, &self.throttle, &url, "/unreadNum").await
    }

    /// GET /api/jwbmessage/group?... — 分组列表。`include_read=false` 仅返有未读的（后端语义）。
    pub async fn groups(
        &self,
        page: u32,
        page_size: u32,
        include_read: bool,
    ) -> Result<(u32, Vec<Group>)> {
        let url = format!(
            "{BASE}/api/jwbmessage/group?key=&page={page}&pageSize={page_size}&read={include_read}"
        );
        let env: GroupEnvelope = fetch_json(&self.http, &self.throttle, &url, "/group").await?;
        Ok((env.total, env.entities))
    }

    /// GET /api/jwbmessage/messagelist?... — 组内消息列表。
    ///
    /// ⚠ **隐式副作用**：该 GET 会把该分组下所有未读静默标记为已读（实测 §2.5）。
    /// CLI 的 `sjtu messages show` 使用前必须给用户警示。
    pub async fn messages(
        &self,
        group_id: &str,
        is_group: bool,
        page: u32,
        page_size: u32,
        include_read: bool,
    ) -> Result<(u32, Vec<Message>)> {
        let gid = urlencoding(group_id);
        let url = format!(
            "{BASE}/api/jwbmessage/messagelist?page={page}&pageSize={page_size}&key=&groupId={gid}&isGroup={is_group}&read={include_read}"
        );
        let env: MessageListEnvelope =
            fetch_json(&self.http, &self.throttle, &url, "/messagelist").await?;
        Ok((env.total, env.entities))
    }

    /// POST /api/jwbmessage/message/readall — 全部已读（全局，无法按组）。
    pub async fn read_all(&self) -> Result<ReadAllResponse> {
        api_write::read_all(&self.http, &self.throttle, BASE).await
    }
}

/// 最小 URL encoding，同 shuiyuan::urlencoding；这里复制一份避免跨模块 `pub` 泄漏。
pub(super) fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
