//! Canvas Client：connect（读 PAT）+ whoami + planner_items。
//!
//! MVP 只读 3 端点（参见 tasks/s3c-canvas-planner.md §2）：
//! - `/api/v1/users/self` + `/api/v1/users/self/profile`（whoami 合并）
//! - `/api/v1/planner/items?start_date=...[&end_date=...]`（today / upcoming 共用）

use std::sync::Arc;

use anyhow::Result;
use reqwest::Client as HttpClient;

use super::auth::load_pat;
use super::http::{build_http_client, fetch_json, BASE};
use super::models::{PlannerItem, Profile, UserProfile, UserSelf};
use super::throttle::Throttle;

/// Canvas 客户端。每个进程持一实例，复用连接池 + 节流器。
pub struct Client {
    pub(super) http: Arc<HttpClient>,
    pub(super) throttle: Arc<Throttle>,
}

impl Client {
    /// 读 PAT → 构造注入 Bearer 的 HTTP client。
    pub fn connect() -> Result<Self> {
        let pat = load_pat()?;
        let http = build_http_client(&pat)?;
        Ok(Self {
            http: Arc::new(http),
            throttle: Arc::new(Throttle::new()),
        })
    }

    /// GET /users/self + /users/self/profile，合并返回 `Profile`。
    pub async fn whoami(&self) -> Result<Profile> {
        let user_url = format!("{BASE}/api/v1/users/self");
        let profile_url = format!("{BASE}/api/v1/users/self/profile");
        let user: UserSelf =
            fetch_json(&self.http, &self.throttle, &user_url, "/users/self").await?;
        let profile: UserProfile = fetch_json(
            &self.http,
            &self.throttle,
            &profile_url,
            "/users/self/profile",
        )
        .await?;
        Ok(Profile::merge(user, profile))
    }

    /// GET /planner/items —— 聚合 DDL 端点。
    ///
    /// 时间参数是 ISO8601 UTC 字符串（尾 Z）。`end_date=None` 表示一直到未来。
    /// `per_page` 默认值给 100（Canvas 上限），MVP 暂不实现 Link 分页翻页
    /// —— 未来 N 天作业通常 ≤ 50 条，单页已够用。
    pub async fn planner_items(
        &self,
        start_utc: &str,
        end_utc: Option<&str>,
        per_page: u32,
    ) -> Result<Vec<PlannerItem>> {
        let mut url = format!(
            "{BASE}/api/v1/planner/items?start_date={}&order=asc&per_page={per_page}",
            urlencoding(start_utc)
        );
        if let Some(end) = end_utc {
            url.push_str(&format!("&end_date={}", urlencoding(end)));
        }
        let items: Vec<PlannerItem> =
            fetch_json(&self.http, &self.throttle, &url, "/planner/items").await?;
        Ok(items)
    }
}

/// 最小 URL encoding（与 jwbmessage 同源），避免再拉 `url` / `percent-encoding` 依赖链。
fn urlencoding(s: &str) -> String {
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
