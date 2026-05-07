//! 办事大厅 Client：struct + connect + todo 端点。
//!
//! 认证链路：直接复用 S2 `cas::cas_login("services", "https://my.sjtu.edu.cn/ui/app/")`。
//! 主 session 带 JAAuthCookie → 302 链直达 `my.sjtu.edu.cn/ui/app/` → 收 JSESSIONID 等 cookie。
//!
//! sub-session 文件名 `services.json`，与 `jwbmessage.json` 独立（虽同 my.sjtu 后端，
//! 但分离避免一边过期影响另一边的缓存命中）。

use std::sync::Arc;

use anyhow::Result;
use reqwest::Client as HttpClient;

use super::http::{build_http_client, fetch_json, BASE};
use super::models::{TodoEnvelope, TodoItem};
use super::throttle::Throttle;
use crate::auth::cas::cas_login;

/// CAS 跳转的目标：办事大厅入口；未登录时 JAAuthCookie 带走即回跳。
pub(super) const LOGIN_URL: &str = "https://my.sjtu.edu.cn/ui/app/";

/// 办事大厅 Client。
pub struct Client {
    http: HttpClient,
    throttle: Arc<Throttle>,
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
        let r = cas_login("services", LOGIN_URL).await?;
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

    /// GET /api/task/me/processes/todo?thing=false — 待办列表（一次返全部，无分页）。
    ///
    /// 返回 `(total, entities)`。
    /// `total` 与 UI "共 N 条" 对齐；`entities` 内含当前步骤铺顶层 + 嵌套 `process`。
    pub async fn pending(&self) -> Result<(u32, Vec<TodoItem>)> {
        let url = format!("{BASE}/api/task/me/processes/todo?thing=false");
        let env: TodoEnvelope = fetch_json(&self.http, &self.throttle, &url, "/todo").await?;
        Ok((env.total, env.entities))
    }
}
